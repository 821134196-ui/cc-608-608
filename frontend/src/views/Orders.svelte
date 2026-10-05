<script>
  import { onMount } from 'svelte'
  import { api, STATUS_LABELS } from '../api.js'

  let orders = []
  let error = ''

  const STATUS_COLORS = {
    booked: '#e0e7ff|#3730a3',
    confirmed: '#cffafe|#155e75',
    in_progress: '#fef9c3|#854d0e',
    awaiting_acceptance: '#ffedd5|#9a3412',
    accepted: '#dcfce7|#166534',
    closed: '#e5e7eb|#374151',
  }

  function badgeStyle(s) {
    const [bg, fg] = (STATUS_COLORS[s] || '#e5e7eb|#374151').split('|')
    return `background:${bg};color:${fg}`
  }

  onMount(async () => {
    try {
      orders = await api.listOrders()
    } catch (e) {
      error = e.message
    }
  })
</script>

<div class="card">
  <h2>订单列表</h2>
  {#if error}<div class="error-box">{error}</div>{/if}
  {#if orders.length === 0 && !error}
    <p style="color:#64748b;">暂无订单，去 <a href="#/book">客户预约</a> 创建第一单。</p>
  {:else}
    <table>
      <thead>
        <tr><th>#</th><th>客户</th><th>型号</th><th>预约时间</th><th>状态</th><th></th></tr>
      </thead>
      <tbody>
        {#each orders as o}
          <tr>
            <td>{o.id}</td>
            <td>{o.customer_name}</td>
            <td>{o.hood_model}</td>
            <td>{o.appointment_time}</td>
            <td><span class="badge" style={badgeStyle(o.status)}>{STATUS_LABELS[o.status] || o.status}</span></td>
            <td><a href="#/order/{o.id}">进入订单 →</a></td>
          </tr>
        {/each}
      </tbody>
    </table>
  {/if}
</div>
