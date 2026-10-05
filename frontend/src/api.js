const BASE = '/api'

async function request(method, url, body) {
  const opts = { method, headers: {} }
  if (body !== undefined) {
    opts.headers['Content-Type'] = 'application/json'
    opts.body = JSON.stringify(body)
  }
  const res = await fetch(BASE + url, opts)
  const data = await res.json().catch(() => ({}))
  if (!res.ok) throw new Error(data.error || `请求失败 (${res.status})`)
  return data
}

export const api = {
  listOrders: () => request('GET', '/orders'),
  createOrder: (o) => request('POST', '/orders', o),
  getOrder: (id) => request('GET', `/orders/${id}`),
  confirmScope: (id, b) => request('POST', `/orders/${id}/confirm-scope`, b),
  startWork: (id) => request('POST', `/orders/${id}/start`),
  addPart: (id, b) => request('POST', `/orders/${id}/parts`, b),
  reinstallPart: (id, pid) => request('POST', `/orders/${id}/parts/${pid}/reinstall`),
  deletePart: (id, pid) => request('DELETE', `/orders/${id}/parts/${pid}`),
  capture: (id, phase) => request('POST', `/orders/${id}/measurements/capture`, { phase }),
  proposeExtra: (id, b) => request('POST', `/orders/${id}/extras`, b),
  decideExtra: (id, eid, decision) => request('POST', `/orders/${id}/extras/${eid}/${decision}`),
  complete: (id) => request('POST', `/orders/${id}/complete`),
  accept: (id, key) => request('POST', `/orders/${id}/accept`, { idempotency_key: key }),
  pay: (id, key) => request('POST', `/orders/${id}/pay`, { idempotency_key: key }),
  uploadPhoto: async (id, kind, file) => {
    const fd = new FormData()
    fd.append('file', file)
    const res = await fetch(`${BASE}/orders/${id}/photos?kind=${kind}`, { method: 'POST', body: fd })
    const data = await res.json().catch(() => ({}))
    if (!res.ok) throw new Error(data.error || '上传失败')
    return data
  },
}

export const STATUS_LABELS = {
  booked: '已预约',
  confirmed: '已确认范围',
  in_progress: '清洗中',
  awaiting_acceptance: '待验收',
  accepted: '已验收',
  closed: '已结单',
}

export function fenToYuan(cents) {
  return `¥${(cents / 100).toFixed(2)}`
}
