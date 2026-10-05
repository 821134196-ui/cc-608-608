<script>
  import { onMount } from 'svelte'
  import { api, STATUS_LABELS, fenToYuan } from '../api.js'
  import TechPanel from '../components/TechPanel.svelte'
  import CustomerPanel from '../components/CustomerPanel.svelte'

  export let id

  let order = null
  let error = ''
  let tab = 'tech'

  async function reload() {
    try {
      order = await api.getOrder(id)
      error = ''
    } catch (e) {
      error = e.message
    }
  }

  onMount(reload)
</script>

{#if error}
  <div class="error-box">{error}</div>
{/if}

{#if order}
  <div class="card">
    <div style="display:flex; justify-content:space-between; align-items:flex-start; flex-wrap:wrap; gap:8px;">
      <div>
        <h2 style="margin-bottom:4px;">订单 #{order.id} · {order.customer_name}</h2>
        <div style="font-size:14px; color:#64748b;">
          {order.phone} · {order.address}<br />
          预约型号：{order.hood_model}
          {#if order.confirmed_model}
            · 现场确认：<strong>{order.confirmed_model}</strong>
            {#if order.confirmed_model !== order.hood_model}
              <span class="badge" style="background:#fee2e2;color:#b91c1c;">型号不符</span>
            {/if}
          {/if}
          <br />预约时间：{order.appointment_time}
        </div>
        {#if order.scope_note}
          <div style="font-size:13px; color:#9a3412; margin-top:6px;">现场备注：{order.scope_note}</div>
        {/if}
      </div>
      <div style="text-align:right;">
        <span class="badge" style="background:#e0e7ff;color:#3730a3; font-size:14px;">
          {STATUS_LABELS[order.status] || order.status}
        </span>
        <div style="margin-top:8px; font-size:14px;">
          应付合计 <strong style="font-size:20px;">{fenToYuan(order.bill.total_cents)}</strong>
          {#if order.bill.frozen}<span class="badge" style="background:#e5e7eb;color:#374151;">已冻结</span>{/if}
        </div>
      </div>
    </div>
  </div>

  <div style="margin-bottom:14px;">
    <button class={tab === 'tech' ? '' : 'secondary'} on:click={() => (tab = 'tech')}>🔧 师傅施工端</button>
    <button class={tab === 'customer' ? '' : 'secondary'} on:click={() => (tab = 'customer')}>🧾 客户验收端</button>
  </div>

  {#if tab === 'tech'}
    <TechPanel {order} on:changed={reload} />
  {:else}
    <CustomerPanel {order} on:changed={reload} />
  {/if}
{:else if !error}
  <p>加载中…</p>
{/if}
