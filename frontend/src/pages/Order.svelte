<script>
  import { onMount } from 'svelte';
  import { api, STATUS_LABELS, yuan, fmtTime } from '../api.js';
  import PhotoUploader from '../components/PhotoUploader.svelte';
  import PartsPanel from '../components/PartsPanel.svelte';
  import MeterPanel from '../components/MeterPanel.svelte';
  import AddonsPanel from '../components/AddonsPanel.svelte';

  export let orderId;
  export let role;

  let d = null;
  let loading = true;
  let error = '';
  let notice = '';
  let tab = 'work';

  // 师傅到场
  let actualModel = '';
  let arriveNote = '';
  let arriving = false;

  let working = false; // 提交完工
  let accepting = false;
  let rejecting = false;
  let paying = false;
  let starting = false;

  async function reload() {
    try {
      d = await api.order(orderId);
      if (!actualModel) actualModel = d.model;
    } catch (e) {
      error = e.message;
    } finally {
      loading = false;
    }
  }

  function flash(msg, isError = false) {
    notice = { msg, isError };
    setTimeout(() => (notice = ''), 5000);
  }

  async function run(fn, okMsg) {
    error = '';
    try {
      await fn();
      if (okMsg) flash(okMsg);
      await reload();
    } catch (e) {
      flash(e.message, true);
    }
  }

  async function arrive() {
    if (!actualModel.trim()) {
      flash('请填写现场核对的型号', true);
      return;
    }
    arriving = true;
    try {
      const r = await api.arrive(orderId, {
        actual_model: actualModel.trim(),
        scope_note: arriveNote.trim(),
      });
      if (r.started) {
        flash('范围确认一致，已开工');
      } else {
        flash(r.warning, true);
        tab = 'addons';
      }
      await reload();
    } catch (e) {
      flash(e.message, true);
    } finally {
      arriving = false;
    }
  }

  async function startWork() {
    starting = true;
    await run(() => api.startWork(orderId));
    starting = false;
  }

  async function completeWork() {
    if (!window.confirm('提交完工后将进入客户验收。请确认：所有零件已复装、前后照片与风量读数已齐备。')) return;
    working = true;
    await run(() => api.completeWork(orderId), '已提交完工，等待客户核对验收');
    tab = 'accept';
    working = false;
  }

  async function accept() {
    const msg = `请客户最终确认：\n· 清洗前/后照片已核对\n· 风量读数已核对\n· 复装清单 ${d.parts.length} 件全部齐套\n· 应付金额 ${yuan(d.bill.total_cents)}\n确认验收后即可结单。`;
    if (!window.confirm(msg)) return;
    accepting = true;
    await run(() => api.submitAcceptance(orderId, true, '客户核对前后照片/风量/复装清单后确认'), '验收通过 ✅');
    accepting = false;
  }

  async function rejectAcceptance() {
    const note = window.prompt('请简要填写需要整改的问题（订单将退回师傅施工）：', '');
    if (note === null) return;
    rejecting = true;
    await run(() => api.submitAcceptance(orderId, false, note), '已驳回，订单退回整改');
    rejecting = false;
  }

  async function pay() {
    if (!window.confirm(`确认收款 ${yuan(d.bill.total_cents)} 并结单？收款只能办理一次。`)) return;
    paying = true;
    try {
      const r = await api.pay(orderId, '现场收款');
      flash(`结单成功，收款 ${yuan(r.amount_cents)}（含基础费 ${yuan(r.breakdown.base_fee_cents)} + 加价 ${yuan(r.breakdown.addon_fee_cents)}）`);
      await reload();
    } catch (e) {
      flash(e.message, true);
    } finally {
      paying = false;
    }
  }

  function statusClass(s) {
    return {
      pending: 'gray',
      in_progress: 'blue',
      awaiting_acceptance: 'amber',
      accepted: 'green',
      completed: 'green',
    }[s] || 'gray';
  }

  $: photosBefore = d ? d.photos.filter((p) => p.phase === 'before') : [];
  $: photosAfter = d ? d.photos.filter((p) => p.phase === 'after') : [];
  $: mBefore = d ? d.measurements.find((m) => m.phase === 'before') : null;
  $: mAfter = d ? d.measurements.find((m) => m.phase === 'after') : null;
  $: pendingAddons = d ? d.addons.filter((a) => a.status === 'pending') : [];
  $: locked = d ? d.status === 'accepted' || d.status === 'completed' : false;

  onMount(reload);
</script>

{#if loading}
  <p class="muted">加载订单…</p>
{:else if !d}
  <div class="alert error">{error || '订单加载失败'}</div>
  <a class="btn ghost" href="#/">返回列表</a>
{:else}
  <!-- 订单头 -->
  <div class="card">
    <div class="row" style="align-items:center">
      <div style="flex:3;min-width:260px">
        <h2 style="margin-bottom:2px">
          {d.model}
          {#if d.actual_model && d.actual_model !== d.model}
            <span class="muted small">→ 现场型号：{d.actual_model}</span>
          {/if}
        </h2>
        <div class="muted small">
          {d.customer_name} · {d.customer_phone} · 预约 {fmtTime(d.appointment_at)}
          {#if d.scope_note} · 范围：{d.scope_note}{/if}
        </div>
        <div class="muted small">订单号 {d.id.slice(0, 12)}…</div>
      </div>
      <div style="flex:1;min-width:150px;text-align:right">
        <span class="badge {statusClass(d.status)}">{STATUS_LABELS[d.status]}</span>
        <div class="small muted" style="margin-top:6px">
          当前视图：{role === 'customer' ? '客户' : '师傅'}
        </div>
      </div>
    </div>
    {#if notice}
      <div class="alert {notice.isError ? 'error' : 'ok'}">{notice.msg}</div>
    {/if}
  </div>

  <!-- 阶段一：师傅到场确认范围 -->
  {#if d.status === 'pending'}
    <div class="card">
      <h2>到场确认 · 型号与施工范围核对</h2>
      <p class="hint">
        师傅上门后先核对现场型号与预约是否一致。型号一致即可按原范围开工；
        <strong>型号不符时系统会生成新方案给客户确认，未经客户确认不能按新方案施工或加价。</strong>
      </p>

      {#if pendingAddons.length > 0}
        <div class="alert warn">
          现场型号与预约不符，已生成方案，正等待客户确认。客户同意后按新方案开工；客户拒绝则仍按原范围执行。
        </div>
        <AddonsPanel orderId={orderId} addons={d.addons} {role} />
        <div style="margin-top:14px">
          {#if role === 'technician'}
            <button on:click={startWork} disabled={starting || pendingAddons.length > 0}>
              按客户决定开工
            </button>
            <p class="muted small">客户在“客户视图”完成同意/拒绝后，师傅点此开工；有待确认方案时按钮不可用。</p>
          {:else}
            <p class="muted small">请在上方对方案选择“同意”或“拒绝”；决定后师傅即可开工。</p>
          {/if}
        </div>
      {:else}
        {#if role === 'customer'}
          <h3>客户上传现场照片（清洗前）</h3>
          <p class="hint">可在师傅上门前上传油烟机现状照片，作为清洗前对比依据。</p>
          <PhotoUploader orderId={orderId} photos={d.photos} phase="before" on:changed={reload} />
        {/if}
        <div class="row">
          <label class="field">
            <span>现场实际型号 *</span>
            <input type="text" bind:value={actualModel} placeholder="核对机身铭牌型号" />
          </label>
          <label class="field">
            <span>施工范围说明</span>
            <input type="text" bind:value={arriveNote} placeholder="如：风轮拆洗+油网清洁，不含管道" />
          </label>
        </div>
        {#if role === 'technician'}
          <button on:click={arrive} disabled={arriving}>{arriving ? '确认中…' : '确认范围并开工'}</button>
        {:else}
          <p class="muted small">等待师傅到场核对型号与范围。</p>
        {/if}
      {/if}
    </div>
  {/if}

  <!-- 施工中 / 待验收 / 已验收：选项卡 -->
  {#if d.status !== 'pending'}
    <div class="tabs">
      <button class={tab === 'work' ? 'active' : ''} on:click={() => (tab = 'work')}>施工记录</button>
      <button class={tab === 'addons' ? 'active' : ''} on:click={() => (tab = 'addons')}>
        方案与加价{pendingAddons.length ? `（${pendingAddons.length} 待确认）` : ''}
      </button>
      <button class={tab === 'accept' ? 'active' : ''} on:click={() => (tab = 'accept')}>
        完工验收{d.status === 'awaiting_acceptance' ? ' · 待客户核对' : ''}
      </button>
    </div>

    {#if tab === 'work'}
      <div class="card">
        <h2>零件拆装登记</h2>
        <p class="hint">师傅每拆下一个零件登记一件，复装一件勾一件；复装清单是验收的硬性条件。</p>
        <PartsPanel orderId={orderId} parts={d.parts} disabled={locked || role === 'customer'} on:changed={reload} />
      </div>

      <div class="card">
        <h2>清洗前 / 清洗后照片</h2>
        <div class="compare">
          <div class="col">
            <div class="col-head"><span>清洗前</span><span class="badge gray">BEFORE</span></div>
            <PhotoUploader orderId={orderId} photos={d.photos} phase="before" disabled={locked || role === 'customer'} on:changed={reload} />
          </div>
          <div class="col">
            <div class="col-head"><span>清洗后</span><span class="badge green">AFTER</span></div>
            <PhotoUploader orderId={orderId} photos={d.photos} phase="after" disabled={locked || d.status === 'awaiting_acceptance' && role === 'technician'} on:changed={reload} />
          </div>
        </div>
      </div>

      <div class="card">
        <h2>风量测量（模拟测量仪）</h2>
        <p class="hint">拆装前后各测一次同一工况点；读数由模拟仪表接口提供，洗后风量应明显高于洗前。</p>
        <MeterPanel orderId={orderId} measurements={d.measurements} disabled={locked || role === 'customer'} on:changed={reload} />
      </div>

      {#if d.status === 'in_progress' && role === 'technician'}
        <div class="card">
          <h2>完工提交</h2>
          <p class="hint">
            提交前系统将强制检查：零件全部复装、前后照片齐全、前后风量齐全且洗后高于洗前、无待确认加价方案。
          </p>
          <button class="amber" on:click={completeWork} disabled={working}>
            {working ? '提交中…' : '施工完成，提交客户验收'}
          </button>
        </div>
      {/if}
    {/if}

    {#if tab === 'addons'}
      <div class="card">
        <h2>新增收费项目 / 方案变更</h2>
        <p class="hint">
          现场需要增加收费时，师傅先提出方案与价格；<strong>未经客户确认不会施工、不会出现在账单中</strong>；
          客户拒绝后仍按原范围执行。
        </p>
        <AddonsPanel orderId={orderId} addons={d.addons} {role} {locked} on:changed={reload} />
      </div>
    {/if}

    {#if tab === 'accept'}
      <!-- 完工验收页：并排照片 + 风量 + 复装清单 + 账单 -->
      <div class="card">
        <h2>完工验收单</h2>
        <p class="hint">
          请客户逐项核对以下内容。零件未完整复装、照片或风量缺失时，系统不允许通过验收。
        </p>

        <h3>① 清洗前 / 清洗后照片对比</h3>
        <div class="compare">
          <div class="col">
            <div class="col-head"><span>清洗前</span><span class="badge gray">{photosBefore.length} 张</span></div>
            <div class="photo-grid">
              {#each photosBefore as p (p.id)}
                <figure>
                  <img class="photo-thumb" src={`/uploads/${p.rel_path}`} alt={p.label} />
                  <figcaption>{p.label || '清洗前'}</figcaption>
                </figure>
              {:else}<p class="muted small">缺照片</p>{/each}
            </div>
          </div>
          <div class="col">
            <div class="col-head"><span>清洗后</span><span class="badge green">{photosAfter.length} 张</span></div>
            <div class="photo-grid">
              {#each photosAfter as p (p.id)}
                <figure>
                  <img class="photo-thumb" src={`/uploads/${p.rel_path}`} alt={p.label} />
                  <figcaption>{p.label || '清洗后'}</figcaption>
                </figure>
              {:else}<p class="muted small">缺照片</p>{/each}
            </div>
          </div>
        </div>

        <h3>② 风量读数对比（m³/h）</h3>
        <div class="compare">
          <div class="col">
            <div class="metric-big">{#if mBefore}{mBefore.airflow_m3h.toFixed(1)}<small> m³/h</small>{:else}<span class="muted">缺读数</span>{/if}</div>
            <div class="muted small">清洗前 · {mBefore ? mBefore.source : ''} · {mBefore ? fmtTime(mBefore.recorded_at) : ''}</div>
          </div>
          <div class="col">
            <div class="metric-big">
              {#if mAfter}{mAfter.airflow_m3h.toFixed(1)}<small> m³/h</small>{:else}<span class="muted">缺读数</span>{/if}
            </div>
            <div class="muted small">清洗后 · {mAfter ? mAfter.source : ''} · {mAfter ? fmtTime(mAfter.recorded_at) : ''}</div>
            {#if mBefore && mAfter && mAfter.airflow_m3h > mBefore.airflow_m3h}
              <span class="badge green" style="margin-top:6px">
                提升 +{(mAfter.airflow_m3h - mBefore.airflow_m3h).toFixed(1)} m³/h（
                {((mAfter.airflow_m3h - mBefore.airflow_m3h) / mBefore.airflow_m3h * 100).toFixed(1)}%）
              </span>
            {:else if mBefore && mAfter}
              <span class="badge red" style="margin-top:6px">洗后未高于洗前，需返工检查</span>
            {/if}
          </div>
        </div>

        <h3>③ 复装清单（共 {d.parts.length} 件）</h3>
        {#if d.missing_parts.length > 0}
          <div class="alert error">
            缺件 {d.missing_parts.length} 个：{d.missing_parts.join('、')} —— 零件没有完整复装，验收无法通过。
          </div>
        {:else if d.parts.length > 0}
          <div class="alert ok">✅ 全部 {d.parts.length} 个拆下零件均已复装，清单完整。</div>
        {:else}
          <div class="alert warn">尚未登记任何零件，不能验收。</div>
        {/if}
        <ul class="parts">
          {#each d.parts as p (p.id)}
            <li>
              <span class="dot {p.reinstalled_at ? 'ok' : 'miss'}"></span>
              <span style="flex:1">{p.name}</span>
              {#if p.reinstalled_at}
                <span class="badge green">已复装 {fmtTime(p.reinstalled_at).slice(5)}</span>
              {:else}
                <span class="badge red">未复装</span>
              {/if}
            </li>
          {/each}
        </ul>

        <h3>④ 费用确认</h3>
        {#each d.bill.lines as line}
          <div class="bill-line">
            <span>
              {line.title}
              {#if line.kind === 'addon'}<span class="badge green" style="margin-left:6px">客户已确认并施工</span>{/if}
            </span>
            <span class="mono">{yuan(line.price_cents)}</span>
          </div>
        {/each}
        <div class="bill-total">
          <span>应付合计</span>
          <span>{yuan(d.bill.total_cents)}</span>
        </div>
        {#if d.addons.filter((a) => a.status !== 'approved' || !a.applied).length > 0}
          <p class="muted small" style="margin-top:8px">
            注：待客户确认或已拒绝的项目均不计入本账单
            （{d.addons.filter((a) => a.status === 'pending').map((a) => `待确认·${a.title}`).concat(d.addons.filter((a) => a.status === 'rejected').map((a) => `已拒绝·${a.title}`)).join('；')}）。
          </p>
        {/if}

        <!-- 验收操作 -->
        <div style="margin-top:18px">
          {#if d.status === 'awaiting_acceptance'}
            {#if role === 'customer'}
              <button on:click={accept} disabled={accepting || d.missing_parts.length > 0}>
                {accepting ? '提交中…' : '我已核对，确认验收'}
              </button>
              <button class="danger" style="margin-left:10px" on:click={rejectAcceptance} disabled={rejecting}>
                有问题，驳回整改
              </button>
              {#if d.missing_parts.length > 0}
                <p class="muted small" style="margin-top:8px">存在缺件，“确认验收”按钮已锁定。</p>
              {/if}
            {:else}
              <div class="alert info">已提交完工，等待客户在“客户视图”核对并确认验收。</div>
            {/if}
          {:else if d.status === 'accepted'}
            <div class="alert ok">客户已于 {fmtTime(d.accepted_at)} 确认验收。</div>
            {#if !d.payment}
              <button on:click={pay} disabled={paying}>{paying ? '结单中…' : `确认收款 ${yuan(d.bill.total_cents)} 并结单`}</button>
            {/if}
          {:else if d.status === 'completed'}
            <div class="alert ok">
              ✅ 已结单收款 {yuan(d.payment.amount_cents)}，时间 {fmtTime(d.payment.created_at)}。
              验收与收款均只办理了一次。
            </div>
          {:else}
            <p class="muted small">施工进行中，完工后由师傅提交，客户再在此页验收。</p>
          {/if}
        </div>
      </div>
    {/if}
  {/if}

  <p style="margin-top:20px"><a href="#/">← 返回订单列表</a></p>
{/if}
