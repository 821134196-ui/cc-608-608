<script>
  import { createEventDispatcher } from 'svelte'
  import { api, fenToYuan } from '../api.js'

  export let order
  const dispatch = createEventDispatcher()

  let error = ''
  let notice = ''
  let busy = false

  // 幂等键：页面加载时生成一次。网络重试 / 重复点击都复用同一个键，
  // 后端凭它保证「重复提交只办理一次」。
  const acceptKey = crypto.randomUUID()
  const payKey = crypto.randomUUID()

  async function run(fn, okMsg) {
    error = ''
    notice = ''
    busy = true
    try {
      await fn()
      if (okMsg) notice = okMsg
      dispatch('changed')
    } catch (e) {
      error = e.message
      dispatch('changed') // 状态可能已变化（如已验收），刷新展示
    } finally {
      busy = false
    }
  }

  const decide = (eid, decision) =>
    run(() => api.decideExtra(order.id, eid, decision))

  const accept = () =>
    run(() => api.accept(order.id, acceptKey), '验收成功！')

  const pay = () =>
    run(() => api.pay(order.id, payKey), '结单完成，感谢使用！')

  $: beforePhotos = order.photos.filter((p) => p.kind === 'before')
  $: afterPhotos = order.photos.filter((p) => p.kind === 'after')
  $: beforeM = order.measurements.filter((m) => m.phase === 'before').at(-1)
  $: afterM = order.measurements.filter((m) => m.phase === 'after').at(-1)
  $: airflowDelta = beforeM && afterM ? (afterM.airflow_m3_min - beforeM.airflow_m3_min).toFixed(1) : null
  $: missingParts = order.parts.filter((p) => !p.reinstalled)
  $: canDecideExtras = ['confirmed', 'in_progress', 'awaiting_acceptance'].includes(order.status)
  $: blockers = order.acceptance_blockers || []
</script>

{#if error}<div class="error-box">{error}</div>{/if}
{#if notice}<div class="ok-box">{notice}</div>{/if}

<div class="card">
  <h2>清洗效果对比</h2>
  {#if beforePhotos.length === 0 && afterPhotos.length === 0}
    <p style="color:#64748b; font-size:14px;">师傅尚未上传施工照片。</p>
  {:else}
    <div style="display:grid; grid-template-columns:1fr 1fr; gap:16px;">
      <div>
        <h3 style="font-size:14px; color:#64748b; margin:0 0 8px;">清洗前</h3>
        <div style="display:flex; gap:6px; flex-wrap:wrap;">
          {#each beforePhotos as p}
            <a href={p.url} target="_blank"><img src={p.url} alt="清洗前" style="width:120px; height:120px; object-fit:cover; border-radius:8px;" /></a>
          {:else}
            <span style="color:#94a3b8; font-size:13px;">暂无</span>
          {/each}
        </div>
      </div>
      <div>
        <h3 style="font-size:14px; color:#64748b; margin:0 0 8px;">清洗后</h3>
        <div style="display:flex; gap:6px; flex-wrap:wrap;">
          {#each afterPhotos as p}
            <a href={p.url} target="_blank"><img src={p.url} alt="清洗后" style="width:120px; height:120px; object-fit:cover; border-radius:8px;" /></a>
          {:else}
            <span style="color:#94a3b8; font-size:13px;">暂无</span>
          {/each}
        </div>
      </div>
    </div>
  {/if}

  <div style="display:flex; gap:24px; margin-top:14px; flex-wrap:wrap;">
    <div>清洗前风量：<strong>{beforeM ? `${beforeM.airflow_m3_min} m³/min` : '—'}</strong></div>
    <div>清洗后风量：<strong>{afterM ? `${afterM.airflow_m3_min} m³/min` : '—'}</strong></div>
    {#if airflowDelta !== null}
      <div>变化：<strong style="color:{Number(airflowDelta) >= 0 ? '#16a34a' : '#dc2626'};">
        {Number(airflowDelta) >= 0 ? '+' : ''}{airflowDelta} m³/min
      </strong></div>
    {/if}
  </div>
</div>

<div class="card">
  <h2>零件复装清单</h2>
  {#if order.parts.length === 0}
    <p style="color:#64748b; font-size:14px;">师傅尚未登记拆下零件。</p>
  {:else}
    <table>
      <thead><tr><th>零件</th><th>数量</th><th>复装状态</th><th>复装时间</th></tr></thead>
      <tbody>
        {#each order.parts as p}
          <tr>
            <td>{p.name}</td>
            <td>{p.quantity}</td>
            <td>
              {#if p.reinstalled}
                <span class="badge" style="background:#dcfce7;color:#166534;">✓ 已复装</span>
              {:else}
                <span class="badge" style="background:#fee2e2;color:#b91c1c;">✗ 未复装</span>
              {/if}
            </td>
            <td>{p.reinstalled_at || '—'}</td>
          </tr>
        {/each}
      </tbody>
    </table>
    {#if missingParts.length}
      <div class="error-box" style="margin-bottom:0;">
        还有 {missingParts.length} 项零件未复装，全部复装后才能通过验收。
      </div>
    {/if}
  {/if}
</div>

{#if order.extras.length}
  <div class="card">
    <h2>现场加价项目（需您确认）</h2>
    <p style="font-size:13px; color:#64748b; margin-top:0;">
      只有您点击「同意」的项目才会计入账单；拒绝后师傅将按原范围施工。
    </p>
    <table>
      <thead><tr><th>项目</th><th>价格</th><th>状态</th><th></th></tr></thead>
      <tbody>
        {#each order.extras as ex}
          <tr>
            <td>{ex.description}</td>
            <td>{fenToYuan(ex.price_cents)}</td>
            <td>
              {#if ex.status === 'pending'}<span class="badge" style="background:#fef9c3;color:#854d0e;">待您确认</span>
              {:else if ex.status === 'approved'}<span class="badge" style="background:#dcfce7;color:#166534;">已同意</span>
              {:else}<span class="badge" style="background:#e5e7eb;color:#374151;">已拒绝</span>{/if}
            </td>
            <td style="white-space:nowrap;">
              {#if ex.status === 'pending' && canDecideExtras}
                <button class="success" style="padding:4px 12px; font-size:13px;" on:click={() => decide(ex.id, 'approve')} disabled={busy}>同意</button>
                <button class="secondary" style="padding:4px 12px; font-size:13px;" on:click={() => decide(ex.id, 'reject')} disabled={busy}>拒绝</button>
              {/if}
            </td>
          </tr>
        {/each}
      </tbody>
    </table>
  </div>
{/if}

<div class="card">
  <h2>费用明细</h2>
  <table>
    <tbody>
      <tr><td>基础清洗费</td><td style="text-align:right;">{fenToYuan(order.bill.base_cents)}</td></tr>
      {#each order.extras.filter((e) => e.status === 'approved') as ex}
        <tr><td>加价：{ex.description}</td><td style="text-align:right;">{fenToYuan(ex.price_cents)}</td></tr>
      {/each}
      <tr style="font-weight:700; font-size:16px;">
        <td>应付合计</td><td style="text-align:right;">{fenToYuan(order.bill.total_cents)}</td>
      </tr>
    </tbody>
  </table>
  {#if order.bill.frozen}
    <p style="font-size:12px; color:#64748b; margin-bottom:0;">金额已于验收时冻结，之后任何变更不影响本单费用。</p>
  {/if}
</div>

{#if order.status === 'awaiting_acceptance'}
  <div class="card">
    <h2>确认验收</h2>
    {#if blockers.length}
      <div class="error-box">
        <div style="font-weight:600; margin-bottom:4px;">暂不能验收，待解决：</div>
        <ul style="margin:0; padding-left:18px;">
          {#each blockers as b}<li>{b}</li>{/each}
        </ul>
      </div>
    {/if}
    <p style="font-size:13px; color:#64748b;">
      请核对上方照片、风量读数与零件复装清单。确认无误后点击验收，验收通过才能结单。
    </p>
    <button class="success" on:click={accept} disabled={busy || blockers.length > 0}>
      {busy ? '提交中…' : '✓ 确认验收'}
    </button>
  </div>
{/if}

{#if order.status === 'accepted'}
  <div class="card">
    <h2>支付结单</h2>
    <p style="font-size:14px; color:#64748b; margin-top:0;">
      验收已于 {order.acceptance.confirmed_at} 完成，应付 <strong>{fenToYuan(order.bill.total_cents)}</strong>。
    </p>
    <button on:click={pay} disabled={busy}>{busy ? '支付中…' : `支付 ${fenToYuan(order.bill.total_cents)} 并结单`}</button>
  </div>
{/if}

{#if order.status === 'closed'}
  <div class="ok-box" style="font-size:15px;">
    ✅ 订单已完成：验收于 {order.acceptance.confirmed_at}，收款 {fenToYuan(order.payment.amount_cents)} 于 {order.payment.paid_at}。感谢惠顾！
  </div>
{/if}

{#if ['booked', 'confirmed', 'in_progress'].includes(order.status)}
  <div class="card">
    <p style="color:#64748b; font-size:14px; margin:0;">
      师傅正在施工中（当前状态：{order.status === 'booked' ? '已预约' : order.status === 'confirmed' ? '已确认范围' : '清洗中'}），完工后此处将展示完整的验收信息。
    </p>
  </div>
{/if}
