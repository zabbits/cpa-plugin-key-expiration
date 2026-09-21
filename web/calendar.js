const pad = (value) => String(value).padStart(2, '0');

// Calendar arithmetic uses local noon so DST changes do not move the date.
function civilDate(year, month, day) {
  const date = new Date(0);
  date.setFullYear(year, month, day);
  date.setHours(12, 0, 0, 0);
  return date;
}

function dateKey(date) {
  return `${String(date.getFullYear()).padStart(4, '0')}-${pad(date.getMonth() + 1)}-${pad(date.getDate())}`;
}

function addMonths(date, months) {
  const last = civilDate(date.getFullYear(), date.getMonth() + months + 1, 0);
  return civilDate(last.getFullYear(), last.getMonth(), Math.min(date.getDate(), last.getDate()));
}

function supported(date) {
  return date.getFullYear() >= 1 && date.getFullYear() <= 9999;
}

const dayLabel = new Intl.DateTimeFormat('zh-CN', { dateStyle: 'full' });

export class DatePicker {
  constructor(root) {
    this.root = root;
    this.grid = root.querySelector('[data-calendar-grid]');
    this.monthLabel = root.querySelector('[data-month-label]');
    this.previous = root.querySelector('[data-previous]');
    this.next = root.querySelector('[data-next]');
    this.timeFields = ['hour', 'minute', 'second'].map((name) => root.querySelector(`[data-${name}]`));
    this.previous.addEventListener('click', () => this.moveMonth(-1));
    this.next.addEventListener('click', () => this.moveMonth(1));
    root.querySelector('[data-today]').addEventListener('click', () => this.select(new Date()));
    this.grid.addEventListener('click', (event) => {
      const day = event.target.closest('button[data-date]');
      if (!day || this.root.disabled) return;
      const [year, month, date] = day.dataset.date.split('-').map(Number);
      this.select(civilDate(year, month - 1, date));
    });
    this.grid.addEventListener('keydown', (event) => this.navigate(event));
    this.timeFields.forEach((field, index) => {
      const max = index === 0 ? 23 : 59;
      field.addEventListener('blur', () => {
        if (/^\d{1,2}$/.test(field.value) && Number(field.value) <= max) field.value = pad(field.value);
      });
      field.addEventListener('keydown', (event) => {
        if (event.key !== 'ArrowUp' && event.key !== 'ArrowDown') return;
        event.preventDefault();
        const current = Number(field.value) || 0;
        field.value = pad((current + (event.key === 'ArrowUp' ? 1 : -1) + max + 1) % (max + 1));
      });
    });
  }

  setValue(value) {
    const [day, time] = value.split('T');
    const [year, month, date] = day.split('-').map(Number);
    this.selected = civilDate(year, month - 1, date);
    this.focused = this.selected;
    this.view = civilDate(year, month - 1, 1);
    const parts = time.split(':');
    this.timeFields.forEach((field, index) => { field.value = parts[index]; });
    this.render();
  }

  getValue() {
    const values = this.timeFields.map((field) => field.value.trim());
    if (values.some((value, index) => !/^\d{1,2}$/.test(value) || Number(value) > (index === 0 ? 23 : 59))) return '';
    return `${dateKey(this.selected)}T${values.map(pad).join(':')}`;
  }

  setDisabled(disabled) {
    this.root.disabled = disabled;
  }

  focus() {
    this.grid.querySelector('button[tabindex="0"]')?.focus();
  }

  select(date) {
    if (!supported(date)) return;
    this.selected = civilDate(date.getFullYear(), date.getMonth(), date.getDate());
    this.focused = this.selected;
    this.view = civilDate(date.getFullYear(), date.getMonth(), 1);
    this.render();
    this.focus();
  }

  moveMonth(amount) {
    const date = addMonths(this.focused, amount);
    if (!supported(date)) return;
    this.focused = date;
    this.view = civilDate(date.getFullYear(), date.getMonth(), 1);
    this.render();
  }

  navigate(event) {
    if (!event.target.closest('button[data-date]') || this.root.disabled) return;
    const date = this.focused;
    const weekday = (date.getDay() + 6) % 7;
    const offsets = { ArrowLeft: -1, ArrowRight: 1, ArrowUp: -7, ArrowDown: 7, Home: -weekday, End: 6 - weekday };
    let next;
    if (Object.hasOwn(offsets, event.key)) {
      next = civilDate(date.getFullYear(), date.getMonth(), date.getDate() + offsets[event.key]);
    } else if (event.key === 'PageUp' || event.key === 'PageDown') {
      next = addMonths(date, (event.key === 'PageUp' ? -1 : 1) * (event.shiftKey ? 12 : 1));
    } else {
      return;
    }
    event.preventDefault();
    if (!supported(next)) return;
    this.focused = next;
    this.view = civilDate(next.getFullYear(), next.getMonth(), 1);
    this.render();
    this.focus();
  }

  render() {
    const year = this.view.getFullYear();
    const month = this.view.getMonth();
    this.monthLabel.textContent = `${year}年${month + 1}月`;
    this.previous.disabled = year === 1 && month === 0;
    this.next.disabled = year === 9999 && month === 11;
    const weekdays = document.createElement('div');
    weekdays.className = 'calendar-week calendar-weekdays';
    weekdays.setAttribute('role', 'row');
    for (const name of ['一', '二', '三', '四', '五', '六', '日']) {
      const label = document.createElement('span');
      label.textContent = name;
      label.setAttribute('role', 'columnheader');
      weekdays.append(label);
    }
    const rows = [weekdays];
    const offset = (this.view.getDay() + 6) % 7;
    const today = dateKey(new Date());
    for (let week = 0; week < 6; week++) {
      const row = document.createElement('div');
      row.className = 'calendar-week';
      row.setAttribute('role', 'row');
      for (let day = 0; day < 7; day++) {
        const date = civilDate(year, month, week * 7 + day + 1 - offset);
        const key = dateKey(date);
        const selected = key === dateKey(this.selected);
        const cell = document.createElement('div');
        cell.setAttribute('role', 'gridcell');
        cell.setAttribute('aria-selected', String(selected));
        const button = document.createElement('button');
        button.type = 'button';
        button.className = 'calendar-day';
        button.classList.toggle('outside', date.getMonth() !== month);
        button.classList.toggle('selected', selected);
        button.textContent = date.getDate();
        button.dataset.date = key;
        button.tabIndex = key === dateKey(this.focused) ? 0 : -1;
        button.disabled = !supported(date);
        button.setAttribute('aria-label', dayLabel.format(date));
        if (key === today) button.setAttribute('aria-current', 'date');
        cell.append(button);
        row.append(cell);
      }
      rows.push(row);
    }
    this.grid.replaceChildren(...rows);
  }
}
