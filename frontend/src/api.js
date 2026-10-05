// 后端 API 封装。所有错误统一提取 {error} 文案抛出。

const BASE = '/api';

async function request(method, path, body) {
  const opts = { method, headers: {} };
  if (body !== undefined) {
    opts.headers['Content-Type'] = 'application/json';
    opts.body = JSON.stringify(body);
  }
  const res = await fetch(BASE + path, opts);
  const data = await res.json().catch(() => ({}));
  if (!res.ok) {
    const err = new Error(data.error || `请求失败（${res.status}）`);
    err.status = res.status;
    throw err;
  }
  return data;
}

export const api = {
  listOrders: () => request('GET', '/orders'),
  createOrder: (payload) => request('POST', '/orders', payload),
  order: (id) => request('GET', `/orders/${id}`),

  // 师傅到场 / 开工 / 完工
  arrive: (id, payload) => request('POST', `/orders/${id}/arrive`, payload),
  startWork: (id) => request('POST', `/orders/${id}/start-work`),
  completeWork: (id, note = '') =>
    request('POST', `/orders/${id}/complete-work`, { note }),

  // 加价方案
  listAddons: (id) => request('GET', `/orders/${id}/addons`),
  proposeAddon: (id, payload) => request('POST', `/orders/${id}/addons`, payload),
  decideAddon: (id, addonId, decision, customer_note = '') =>
    request('POST', `/orders/${id}/addons/${addonId}/decision`, {
      decision,
      customer_note,
    }),
  applyAddon: (id, addonId) =>
    request('POST', `/orders/${id}/addons/${addonId}/apply`),

  // 零件
  listParts: (id) => request('GET', `/orders/${id}/parts`),
  addPart: (id, name, note = '') =>
    request('POST', `/orders/${id}/parts`, { name, note }),
  reinstall: (id, partId) =>
    request('POST', `/orders/${id}/parts/${partId}/reinstall`),

  // 照片
  listPhotos: (id) => request('GET', `/orders/${id}/photos`),
  uploadPhoto: async (id, file, phase, label = '') => {
    const fd = new FormData();
    fd.append('file', file);
    fd.append('phase', phase);
    fd.append('label', label);
    const res = await fetch(`${BASE}/orders/${id}/photos`, { method: 'POST', body: fd });
    const data = await res.json().catch(() => ({}));
    if (!res.ok) throw new Error(data.error || `上传失败（${res.status}）`);
    return data;
  },

  // 风量
  readMeter: (id, phase) => request('GET', `/orders/${id}/meter/${phase}`),
  recordMeasurement: (id, phase, airflow_m3h = null, note = '') =>
    request('POST', `/orders/${id}/measurements`, { phase, airflow_m3h, note }),

  // 验收 / 收款
  submitAcceptance: (id, accepted, note = '') =>
    request('POST', `/orders/${id}/acceptance`, { accepted, note }),
  pay: (id, method = '现场收款') => request('POST', `/orders/${id}/payment`, { method }),
};

export const STATUS_LABELS = {
  pending: '待到场',
  in_progress: '施工中',
  awaiting_acceptance: '待客户验收',
  accepted: '已验收',
  completed: '已结单',
};

export function yuan(cents) {
  return `¥${(Number(cents || 0) / 100).toFixed(2)}`;
}

export function fmtTime(rfc) {
  if (!rfc) return '—';
  const d = new Date(rfc);
  if (Number.isNaN(d.getTime())) return rfc;
  const p = (n) => String(n).padStart(2, '0');
  return `${d.getFullYear()}-${p(d.getMonth() + 1)}-${p(d.getDate())} ${p(d.getHours())}:${p(d.getMinutes())}`;
}
