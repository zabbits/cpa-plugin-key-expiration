import { readPanelCredential } from './panel.js';
import { DatePicker } from './calendar.js';

const $ = (id) => document.getElementById(id);
const labels = { active: '有效', expired: '已过期', disabled: '已禁用' };
const messages = {
  key_not_found: 'Key 已被删除，请刷新',
  key_sync_required: '请刷新页面同步 Key',
  storage_unavailable: '规则数据库不可用',
  unsupported_database: '请升级插件以读取当前数据库',
  invalid_settings: '插件配置无效',
  invalid_request: '请检查填写的内容',
};
const prefix = location.pathname.split('/v0/resource/plugins/')[0];
const endpoint = `${prefix}/v0/management/key-expiration/keys`;
const cpaKeysEndpoint = `${prefix}/v0/management/api-keys`;
const formatter = new Intl.DateTimeFormat('zh-CN', { dateStyle: 'medium', timeStyle: 'medium' });
const datePicker = new DatePicker($('date-picker'));

let credential = readPanelCredential();
let keys = [];
let clockOffset = 0;
let busy = false;
let loading = false;
let editing = null;
let initialDate = '';
let toastTimer;

function showError(error, target = 'error') {
  const element = $(target);
  element.textContent = error?.message || '';
  element.hidden = !error;
}

async function api(path, method = 'GET', body) {
  const headers = { Accept: 'application/json' };
  if (credential) headers.Authorization = `Bearer ${credential}`;
  if (body) headers['Content-Type'] = 'application/json';
  let response;
  try {
    response = await fetch(path, {
      method, headers, body: body ? JSON.stringify(body) : undefined,
      credentials: 'same-origin', cache: 'no-store', redirect: 'error',
    });
  } catch {
    throw new Error('连接失败');
  }
  if (response.status === 401 || response.status === 403) {
    credential = '';
    keys = [];
    $('keys').replaceChildren();
    $('content').hidden = true;
    $('login').hidden = false;
    $('editor').close();
    $('credential').focus();
    throw new Error('请输入管理密钥');
  }
  const data = await response.json().catch(() => null);
  if (!response.ok) {
    throw new Error(messages[data?.error?.code] || `请求失败（${response.status}）`);
  }
  if (!data) throw new Error('服务器响应无效');
  if (data.server_time) clockOffset = Date.parse(data.server_time) - Date.now();
  return data;
}

function statusOf(key) {
  if (key.policy.disabled) return 'disabled';
  return key.policy.expires_at && Date.parse(key.policy.expires_at) <= Date.now() + clockOffset
    ? 'expired' : 'active';
}

function element(tag, text, className) {
  const node = document.createElement(tag);
  if (text !== undefined) node.textContent = text;
  if (className) node.className = className;
  return node;
}

function icon(name) {
  const paths = {
    key: ['M14 4a6 6 0 0 0-5.4 8.6L3 18.2V21h3v-3h3v-3l2.4-2.4A6 6 0 1 0 14 4Z', 'M16.5 7.5h.01'],
    calendar: ['M8 2v4M16 2v4M3 10h18M21 10V6a2 2 0 0 0-2-2H5a2 2 0 0 0-2 2v13a2 2 0 0 0 2 2h5', 'M22 17a5 5 0 1 1-10 0 5 5 0 0 1 10 0ZM17 14v3l1.5 1'],
    pause: ['M6 5h4v14H6Z', 'M14 5h4v14h-4Z'],
    play: ['m8 5 11 7-11 7V5Z'],
  };
  const svg = document.createElementNS('http://www.w3.org/2000/svg', 'svg');
  svg.setAttribute('viewBox', '0 0 24 24');
  svg.setAttribute('class', 'icon');
  svg.setAttribute('aria-hidden', 'true');
  for (const d of paths[name]) {
    const path = document.createElementNS(svg.namespaceURI, 'path');
    path.setAttribute('d', d);
    svg.append(path);
  }
  return svg;
}

function render() {
  const search = $('search').value.trim().toLowerCase();
  const filter = $('filter').value;
  const visible = keys.filter((key) =>
    (filter === 'all' || filter === statusOf(key)) &&
    `${key.key} ${key.id}`.toLowerCase().includes(search));
  const cards = visible.map((key) => {
    const card = element('article', undefined, 'key-card');
    const header = element('header', undefined, 'key-card-header');
    const symbol = element('span', undefined, 'key-symbol');
    symbol.append(icon('key'));
    const identity = element('div', undefined, 'key-identity');
    const title = element('h2', key.key, 'key-value');
    title.id = `key-${key.id}`;
    card.setAttribute('aria-labelledby', title.id);
    identity.append(title);
    // Short keys may have the same masked label; retain a stable discriminator.
    identity.append(element('span', key.id.slice(0, 10), 'key-id'));
    const status = statusOf(key);
    card.dataset.status = status;
    header.append(symbol, identity);
    const badge = element('span', labels[status], `badge ${status}`);
    const expiry = element('dl', undefined, 'key-expiry');
    const expiryValue = element('dd');
    if (key.policy.expires_at) {
      const time = element('time', formatter.format(new Date(key.policy.expires_at)));
      time.dateTime = key.policy.expires_at;
      expiryValue.append(time);
    } else {
      expiryValue.textContent = '永不过期';
    }
    expiry.append(element('dt', '到期时间'), expiryValue);
    const actions = element('div', undefined, 'key-actions');
    actions.setAttribute('role', 'group');
    actions.setAttribute('aria-label', 'Key 操作');
    for (const [action, text] of [['edit', '设置有效期'], ['toggle', key.policy.disabled ? '启用' : '禁用']]) {
      const button = element('button', undefined, 'icon-button');
      button.type = 'button';
      button.title = text;
      button.setAttribute('aria-label', text);
      button.dataset.action = action;
      button.dataset.id = key.id;
      button.disabled = busy || loading;
      button.append(icon(action === 'edit' ? 'calendar' : key.policy.disabled ? 'play' : 'pause'));
      actions.append(button);
    }
    card.append(header, actions, expiry, badge);
    return card;
  });
  $('keys').replaceChildren(...cards);
  $('empty').textContent = keys.length ? '无匹配 Key' : '暂无 Key';
  $('empty').hidden = visible.length > 0;
}

function lock(value) {
  busy = value;
  $('refresh').disabled = busy || loading;
  $('save').disabled = busy;
  $('cancel').disabled = busy;
  $('permanent').disabled = busy;
  datePicker.setDisabled(busy || $('permanent').checked);
  render();
}

async function load() {
  if (busy || loading) return;
  if (!credential) {
    $('loading').hidden = true;
    $('login').hidden = false;
    return;
  }
  loading = true;
  $('refresh').disabled = true;
  showError(null);
  render();
  try {
    const configured = await api(cpaKeysEndpoint);
    // Only a successful, well-formed CPA snapshot may replace the catalog.
    // CPA serializes a nil key slice as null.
    const rawKeys = configured['api-keys'] === null ? [] : configured['api-keys'];
    if (!Array.isArray(rawKeys) || rawKeys.some((key) => typeof key !== 'string')) {
      throw new Error('Key 列表响应无效');
    }
    const data = await api(`${endpoint}/sync`, 'POST', { keys: rawKeys, allow_empty: true });
    keys = data.keys;
    $('login').hidden = true;
    $('content').hidden = false;
  } catch (error) {
    showError(error);
  } finally {
    loading = false;
    $('loading').hidden = true;
    $('refresh').disabled = false;
    render();
  }
}

function toast(message) {
  clearTimeout(toastTimer);
  $('toast').textContent = message;
  $('toast').hidden = false;
  toastTimer = setTimeout(() => { $('toast').hidden = true; }, 2400);
}

async function save(key, changes) {
  const result = await api(endpoint, 'PUT', { id: key.id, ...key.policy, ...changes });
  keys = keys.map((entry) => entry.id === key.id ? result : entry);
  render();
}

function localDate(value) {
  const date = new Date(value);
  const pad = (part) => String(part).padStart(2, '0');
  return `${String(date.getFullYear()).padStart(4, '0')}-${pad(date.getMonth() + 1)}-${pad(date.getDate())}T${pad(date.getHours())}:${pad(date.getMinutes())}:${pad(date.getSeconds())}`;
}

$('keys').addEventListener('click', async (event) => {
  const button = event.target.closest('button[data-action]');
  if (!button || busy || loading) return;
  const key = keys.find((entry) => entry.id === button.dataset.id);
  if (!key) return;
  showError(null);
  if (button.dataset.action === 'edit') {
    editing = key;
    $('editing-key').textContent = key.key;
    $('permanent').checked = !key.policy.expires_at;
    initialDate = key.policy.expires_at ? localDate(key.policy.expires_at) : '';
    const tomorrow = new Date(Date.now() + clockOffset);
    tomorrow.setDate(tomorrow.getDate() + 1);
    tomorrow.setHours(23, 59, 59, 0);
    datePicker.setValue(initialDate || localDate(tomorrow));
    datePicker.setDisabled($('permanent').checked);
    showError(null, 'edit-error');
    $('editor').showModal();
    return;
  }
  lock(true);
  try {
    await save(key, { disabled: !key.policy.disabled });
    toast(key.policy.disabled ? '已启用' : '已禁用');
  } catch (error) {
    showError(error);
  } finally {
    lock(false);
  }
});

$('edit-form').addEventListener('submit', async (event) => {
  event.preventDefault();
  if (!editing || busy) return;
  showError(null, 'edit-error');
  let expiry = null;
  if (!$('permanent').checked) {
    const value = datePicker.getValue();
    const date = new Date(value);
    // Reject nonexistent local times at DST transitions instead of shifting them.
    if (!value || !Number.isFinite(date.getTime()) || localDate(date).slice(0, value.length) !== value) {
      showError(new Error('请选择有效的到期时间'), 'edit-error');
      return;
    }
    expiry = value === initialDate ? editing.policy.expires_at : date.toISOString();
  }
  lock(true);
  try {
    await save(editing, { expires_at: expiry });
    $('editor').close();
    toast('已保存');
  } catch (error) {
    showError(error, $('editor').open ? 'edit-error' : 'error');
  } finally {
    lock(false);
  }
});

$('permanent').addEventListener('change', () => {
  datePicker.setDisabled($('permanent').checked);
  if (!$('permanent').checked) datePicker.focus();
});
$('cancel').addEventListener('click', () => $('editor').close());
$('editor').addEventListener('cancel', (event) => { if (busy) event.preventDefault(); });
$('editor').addEventListener('close', () => { editing = null; });
$('search').addEventListener('input', render);
$('filter').addEventListener('change', render);
$('refresh').addEventListener('click', load);
$('login-form').addEventListener('submit', async (event) => {
  event.preventDefault();
  if (loading) return;
  credential = $('credential').value.trim();
  $('credential').value = '';
  await load();
});
document.addEventListener('visibilitychange', () => {
  if (!document.hidden && !busy && !editing && credential) void load();
});
setInterval(() => {
  if (!document.hidden && !busy && !loading && !$('content').hidden && !editing) {
    const changed = keys.some((key) => key.status !== statusOf(key));
    if (changed) { keys.forEach((key) => { key.status = statusOf(key); }); render(); }
  }
}, 1000);
void load();
