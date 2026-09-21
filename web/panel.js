function storedValue(name) {
  try {
    let raw = localStorage.getItem(name);
    if (!raw) return null;
    const version = ['v2', 'v1'].find((value) => raw.startsWith(`enc::${value}::`));
    if (version) {
      const salt = 'cli-proxy-api-webui::secure-storage';
      const secret = new TextEncoder().encode(version === 'v2'
        ? `${salt}|v2|${location.host}`
        : `${salt}|${location.host}|${navigator.userAgent}`);
      const binary = atob(raw.slice(`enc::${version}::`.length));
      const bytes = Uint8Array.from(binary, (char, index) => char.charCodeAt(0) ^ secret[index % secret.length]);
      raw = new TextDecoder().decode(bytes);
    }
    return JSON.parse(raw);
  } catch {
    return null;
  }
}

export function readPanelCredential() {
  if (window.parent === window) return '';
  const candidates = [storedValue('cli-proxy-auth')?.state?.managementKey, storedValue('managementKey')];
  return candidates.find((value) => typeof value === 'string' && value.trim())?.trim() || '';
}
