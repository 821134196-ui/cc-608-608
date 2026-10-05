<script>
  import { createEventDispatcher } from 'svelte'
  import { api, fenToYuan } from '../api.js'

  export let order
  const dispatch = createEventDispatcher()

  let error = ''
  let busy = false

  // 范围确认表单
  let confirmedModel = ''
  let basePrice = 0
  let scopeNote = ''
  $: if (order && order.status === 'booked' && !confirmedModel) {
    confirmedModel = order.hood_model
    basePrice = order.base_price_cents
  }

  // 零件登记表单
  let partName = ''
  let partQty = 1

  // 加价项目表单
  let extraDesc = ''
  let extraPrice = 1000

  async function run(fn) {
    error = ''
    busy = true
    try {
      await fn()
      dispatch('changed')
    } catch (e) {
      error = e.message
    } finally {
      busy = false
    }
  }

  const confirmScope = () =>
    run(() =>
      api.confirmScope(order.id, {
        confirmed_model: confirmedModel,
        base_price_cents: Math.round(Number(basePrice) || 0),
        scope_note: scopeNote || null,
      })
    )

  const startWork = () => run(() => api.startWork(order.id))

  const addPart = () => {
    if (!partName.trim()) return
    run(async () => {
      await api.addPart(order.id, { name: partName.trim(), quantity: Number(partQty) || 1 })
      partName = ''
      partQty = 1
    })
  }

  const reinstall = (pid) => run(() => api.reinstallPart(order.id, pid))
  const removePart = (pid) => run(() => api.deletePart(order.id, pid))

  async function upload(kind, e) {
    const file = e.target.files && e.target.files[0]
    e.target.value = ''
    if (!file) return
    await run(() => api.uploadPhoto(order.id, kind, file))
  }

  const capture = (phase) => run(() => api.capture(order.id, phase))

  const proposeExtra = () => {
    if (!extraDesc.trim()) return
    run(async () => {
      await api.proposeExtra(order.id, {
        description: extraDesc.trim(),
        price_cents: Math.round(Number(extraPrice) || 0),
      })
      extraDesc = ''
      extraPrice = 1000
    })
  }

  const complete = () => run(() => api.complete(order.id))

  $: canEditParts = ['confirmed', 'in_progress'].includes(order.status)
  $: canOperate = ['confirmed', 'in_progress', 'awaiting_acceptance'].includes(order.status)
  $: beforePhotos = order.photos.filter((p) => p.kind === 'before')
  $: afterPhotos = order.photos.filter((p) => p.kind === 'after')
  $: beforeM = order.measurements.filter((m) => m.phase === 'before').at(-1)
  $: afterM = order.measurements.filter((m) => m.phase === 'after').at(-1)
</script>

{#if error}<div class="error-box">{error}</div>{/if}

{#if order.status === 'booked'}
  <div class="card">
    <h2>① 到场确认施工范围</h2>
    <p style="font-size:14px; color:#64748b; margin-top:0;">
      核对实际机型与预约是否一致；如有不符请如实填写确认型号并在备注中说明，后续如需加价须另提方案由客户确认。
    </p>
    <label>现场确认型号</label>
    <input bind:value={confirmedModel} />
    {#if confirmedModel && confirmedModel !== order.hood_model}
      <div class="error-box" style="background:#fff7ed;border-color:#fed7aa;color:#9a3412;">
        与预约型号「{order.hood_model}」不一致，请在备注中说明情况。
      </div>
    {/if}
    <label>基础清洗价格（分）</label>
    <input type="number" bind:value={basePrice} min="0" step="100" />
    <label>范围 / 型号备注</label>
    <textarea bind:value={scopeNote} rows="2" placeholder="例：实际为老板 27A3，与预约型号不符，按标准范围清洗"></textarea>
    <div style="margin-top:12px;">
      <button on:click={confirmScope} disabled={busy || !confirmedModel.trim()}>确认范围</button>
    </div>
  </div>
{/if}

{#if order.status === 'confirmed'}
  <div class="card">
    <h2>② 开始施工</h2>
    <p style="font-size:14px; color:#64748b; margin-top:0;">范围已确认。点击开工后开始拆装与登记。</p>
    <button on:click={startWork} disabled={busy}>开始施工</button>
  </div>
{/if}

{#if canEditParts || order.parts.length}
  <div class="card">
    <h2>零件拆装登记</h2>
    {#if canEditParts}
      <div style="display:flex; gap:8px; align-items:flex-end; flex-wrap:wrap;">
        <div style="flex:2; min-width:160px;">
          <label>零件名称</label>
          <input bind:value={partName} placeholder="油网 / 风轮 / 油杯…" />
        </div>
        <div style="width:90px;">
          <label>数量</label>
          <input type="number" bind:value={partQty} min="1" />
        </div>
        <button on:click={addPart} disabled={busy || !partName.trim()}>登记拆下</button>
      </div>
    {/if}
    {#if order.parts.length}
      <table style="margin-top:12px;">
        <thead><tr><th>零件</th><th>数量</th><th>拆下时间</th><th>复装状态</th><th></th></tr></thead>
        <tbody>
          {#each order.parts as p}
            <tr>
              <td>{p.name}</td>
              <td>{p.quantity}</td>
              <td>{p.removed_at}</td>
              <td>
                {#if p.reinstalled}
                  <span class="badge" style="background:#dcfce7;color:#166534;">已复装</span>
                {:else}
                  <span class="badge" style="background:#fee2e2;color:#b91c1c;">未复装</span>
                {/if}
              </td>
              <td style="white-space:nowrap;">
                {#if !p.reinstalled && canOperate}
                  <button class="success" style="padding:4px 10px; font-size:13px;" on:click={() => reinstall(p.id)} disabled={busy}>标记复装</button>
                {/if}
                {#if canEditParts}
                  <button class="danger" style="padding:4px 10px; font-size:13px;" on:click={() => removePart(p.id)} disabled={busy}>删除</button>
                {/if}
              </td>
            </tr>
          {/each}
        </tbody>
      </table>
    {:else}
      <p style="color:#64748b; font-size:14px;">尚未登记零件。每拆下一件请立即登记，完工验收以本清单为准。</p>
    {/if}
  </div>
{/if}

{#if canOperate}
  <div class="card">
    <h2>施工照片</h2>
    <div style="display:grid; grid-template-columns:1fr 1fr; gap:16px;">
      <div>
        <label>清洗前照片（{beforePhotos.length}）</label>
        <input type="file" accept="image/*" on:change={(e) => upload('before', e)} disabled={busy} />
        <div style="display:flex; gap:6px; flex-wrap:wrap; margin-top:8px;">
          {#each beforePhotos as p}
            <img src={p.url} alt="清洗前" style="width:88px; height:88px; object-fit:cover; border-radius:8px;" />
          {/each}
        </div>
      </div>
      <div>
        <label>清洗后照片（{afterPhotos.length}）</label>
        <input type="file" accept="image/*" on:change={(e) => upload('after', e)} disabled={busy} />
        <div style="display:flex; gap:6px; flex-wrap:wrap; margin-top:8px;">
          {#each afterPhotos as p}
            <img src={p.url} alt="清洗后" style="width:88px; height:88px; object-fit:cover; border-radius:8px;" />
          {/each}
        </div>
      </div>
    </div>
  </div>

  <div class="card">
    <h2>风量测量（模拟测量仪）</h2>
    <div style="display:flex; gap:12px; align-items:center; flex-wrap:wrap;">
      <button class="secondary" on:click={() => capture('before')} disabled={busy}>测清洗前风量</button>
      <button class="secondary" on:click={() => capture('after')} disabled={busy}>测清洗后风量</button>
      <span style="font-size:14px; color:#64748b;">
        清洗前 {beforeM ? `${beforeM.airflow_m3_min} m³/min` : '—'} ·
        清洗后 {afterM ? `${afterM.airflow_m3_min} m³/min` : '—'}
      </span>
    </div>
  </div>

  <div class="card">
    <h2>现场加价方案</h2>
    <p style="font-size:13px; color:#64748b; margin-top:0;">
      型号不符或需新增项目时，在此提出方案与价格。<strong>未经客户在验收端确认，一律不计入账单。</strong>
    </p>
    <div style="display:flex; gap:8px; align-items:flex-end; flex-wrap:wrap;">
      <div style="flex:2; min-width:180px;">
        <label>项目说明</label>
        <input bind:value={extraDesc} placeholder="例：更换老化止回阀" />
      </div>
      <div style="width:130px;">
        <label>价格（分）</label>
        <input type="number" bind:value={extraPrice} min="1" step="100" />
      </div>
      <button on:click={proposeExtra} disabled={busy || !extraDesc.trim()}>提交方案</button>
    </div>
    {#if order.extras.length}
      <table style="margin-top:12px;">
        <thead><tr><th>项目</th><th>价格</th><th>状态</th></tr></thead>
        <tbody>
          {#each order.extras as ex}
            <tr>
              <td>{ex.description}</td>
              <td>{fenToYuan(ex.price_cents)}</td>
              <td>
                {#if ex.status === 'pending'}<span class="badge" style="background:#fef9c3;color:#854d0e;">待客户确认</span>
                {:else if ex.status === 'approved'}<span class="badge" style="background:#dcfce7;color:#166534;">客户已同意</span>
                {:else}<span class="badge" style="background:#e5e7eb;color:#374151;">客户已拒绝</span>{/if}
              </td>
            </tr>
          {/each}
        </tbody>
      </table>
    {/if}
  </div>
{/if}

{#if order.status === 'in_progress'}
  <div class="card">
    <h2>③ 完工提交</h2>
    <p style="font-size:14px; color:#64748b; margin-top:0;">
      提交前请确认：前后照片已上传、前后风量已测量、所有拆下零件已登记。提交后进入客户验收环节。
    </p>
    <button on:click={complete} disabled={busy}>完工，提交验收</button>
  </div>
{/if}

{#if order.status === 'awaiting_acceptance'}
  <div class="ok-box">已完工，等待客户在「客户验收端」核对并确认验收。</div>
{/if}
{#if order.status === 'accepted'}
  <div class="ok-box">客户已确认验收，等待结单。</div>
{/if}
{#if order.status === 'closed'}
  <div class="ok-box">订单已结单完成。✅</div>
{/if}
