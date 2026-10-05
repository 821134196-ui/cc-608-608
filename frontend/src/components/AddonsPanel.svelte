<script>
  import { createEventDispatcher } from 'svelte';
  import { api, yuan, fmtTime } from '../api.js';

  export let orderId;
  export let addons = [];
  export let role; // customer | technician
  export let locked = false; // 已验收/结单后只读

  const dispatch = createEventDispatcher();
  let busy = '';
  let error = '';
  let ok = '';

  const CODE_OPTIONS = [
    { code: 'DEEP_WHEEL', title: '风轮深度拆洗', dft: 8000 },
    { code: 'OIL_PAN', title: '油网/油杯更换', dft: 4500 },
    { code: 'SEAL_KIT', title: '密封件更换', dft: 3000 },
    { code: 'EXTRA_SERVICE', title: '其他增值服务', dft: 5000 },
  ];

  let draft = { code: 'DEEP_WHEEL', title: '风轮深度拆洗', description: '', price_yuan: 80 };
  let proposing = false;

  function onCodeChange(e) {
    const opt = CODE_OPTIONS.find((o) => o.code === e.target.value);
    draft.code = opt.code;
    draft.title = opt.title;
    draft.price_yuan = opt.dft / 100;
  }

  async function propose() {
    error = '';
    ok = '';
    if (!draft.title.trim()) {
      error = '请填写方案名称';
      return;
    }
    proposing = true;
    try {
      await api.proposeAddon(orderId, {
        code: draft.code,
        title: draft.title.trim(),
        description: draft.description.trim(),
        price_cents: Math.round(Number(draft.price_yuan || 0) * 100),
      });
      ok = '方案已提交给客户，等待客户确认。客户确认前不会施工、不计费。';
      draft.description = '';
      dispatch('changed');
    } catch (e) {
      error = e.message;
    } finally {
      proposing = false;
    }
  }

  async function decide(a, decision) {
    busy = a.id + decision;
    error = '';
    ok = '';
    const word = decision === 'approved' ? '同意' : '拒绝';
    if (!window.confirm(`请客户确认：是否${word}「${a.title}」（${yuan(a.price_cents)}）？${decision === 'rejected' ? '拒绝后师傅将按原范围继续施工，不收取该费用。' : ''}`)) {
      busy = '';
      return;
    }
    try {
      await api.decideAddon(orderId, a.id, decision);
      ok = decision === 'approved' ? '已同意该方案。' : '已拒绝该方案，师傅将按原范围执行。';
      dispatch('changed');
    } catch (e) {
      error = e.message;
    } finally {
      busy = '';
    }
  }

  async function apply(a) {
    busy = a.id + 'apply';
    error = '';
    ok = '';
    try {
      await api.applyAddon(orderId, a.id);
      ok = `已按「${a.title}」完成施工，该项目将计入账单。`;
      dispatch('changed');
    } catch (e) {
      error = e.message;
    } finally {
      busy = '';
    }
  }

  function statusBadge(a) {
    if (a.status === 'pending') return ['amber', '待客户确认'];
    if (a.status === 'rejected') return ['red', '客户已拒绝 · 不施工不计费'];
    if (a.status === 'approved' && !a.applied) return ['blue', '客户已同意 · 待施工'];
    return ['green', '已确认并施工 · 计入账单'];
  }
</script>

{#if error}<div class="alert error">{error}</div>{/if}
{#if ok}<div class="alert ok">{ok}</div>{/if}

<table class="list">
  <thead>
    <tr>
      <th>收费项目 / 方案说明</th>
      <th style="width:110px">加价</th>
      <th style="width:190px">状态</th>
      <th style="width:230px">操作</th>
    </tr>
  </thead>
  <tbody>
    {#each addons as a (a.id)}
      <tr>
        <td>
          <strong>{a.title}</strong>
          <div class="muted small">{a.description || '—'}</div>
          <div class="muted small">提出于 {fmtTime(a.created_at)}</div>
        </td>
        <td class="mono">{a.price_cents > 0 ? yuan(a.price_cents) : '差价待定'}</td>
        <td><span class="badge {statusBadge(a)[0]}">{statusBadge(a)[1]}</span></td>
        <td>
          {#if role === 'customer' && a.status === 'pending' && !locked}
            <button class="sm" disabled={busy === a.id + 'approved'} on:click={() => decide(a, 'approved')}>
              同意
            </button>
            <button
              class="sm danger"
              style="margin-left:6px"
              disabled={busy === a.id + 'rejected'}
              on:click={() => decide(a, 'rejected')}
            >
              拒绝（按原范围）
            </button>
          {:else if role === 'technician' && a.status === 'approved' && !a.applied && !locked}
            <button class="sm" disabled={busy === a.id + 'apply'} on:click={() => apply(a)}>
              确认已按方案施工
            </button>
          {:else}
            <span class="muted small">
              {a.status === 'pending' ? '等待客户决定' : a.status === 'rejected' ? `拒绝于 ${fmtTime(a.decided_at)}` : a.applied ? `施工于 ${fmtTime(a.applied_at)}` : '等待施工'}
            </span>
          {/if}
        </td>
      </tr>
    {:else}
      <tr><td colspan="4" class="muted">暂无加价/改方案项目，按原预约范围执行。</td></tr>
    {/each}
  </tbody>
</table>

{#if role === 'technician' && !locked}
  <h3>师傅发起新方案（需客户确认后才能施工）</h3>
  <div class="row">
    <label class="field" style="min-width:200px">
      <span>项目类型</span>
      <select value={draft.code} on:change={onCodeChange}>
        {#each CODE_OPTIONS as o}
          <option value={o.code}>{o.title}</option>
        {/each}
      </select>
    </label>
    <label class="field" style="min-width:200px">
      <span>方案名称</span>
      <input type="text" bind:value={draft.title} />
    </label>
    <label class="field" style="min-width:140px">
      <span>加价金额（元）</span>
      <input type="number" min="0" step="1" bind:value={draft.price_yuan} />
    </label>
  </div>
  <label class="field">
    <span>方案与价格说明（向客户讲清施工范围与原因）</span>
    <textarea rows="2" bind:value={draft.description} placeholder="如：风轮油污严重、常规清洗无法去除，需拆下深度浸泡，加收 ¥80"></textarea>
  </label>
  <button class="sm amber" on:click={propose} disabled={proposing}>
    {proposing ? '提交中…' : '提交方案给客户确认'}
  </button>
  <p class="muted small" style="margin-top:8px">
    规则：只能先报价、后施工。客户未点“同意”前，该项目不会出现在账单里；客户拒绝后仍按原范围执行。
  </p>
{/if}
