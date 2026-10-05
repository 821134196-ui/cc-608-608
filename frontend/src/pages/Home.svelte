<script>
  import { onMount } from 'svelte';
  import { api, STATUS_LABELS, yuan, fmtTime } from '../api.js';

  export let role;

  let orders = [];
  let loading = true;
  let loadError = '';
  let submitting = false;
  let formError = '';
  let createdId = '';

  let form = {
    model: '',
    customer_name: '',
    customer_phone: '',
    appointment_at: '',
    base_fee_cents: 19800,
    scope_note: '',
  };

  async function load() {
    loading = true;
    loadError = '';
    try {
      const data = await api.listOrders();
      orders = data.orders;
    } catch (e) {
      loadError = e.message;
    } finally {
      loading = false;
    }
  }

  async function submit() {
    formError = '';
    createdId = '';
    if (!form.model.trim() || !form.customer_name.trim() || !form.customer_phone.trim()) {
      formError = '请填写型号、联系人与手机号';
      return;
    }
    if (!form.appointment_at) {
      formError = '请选择预约时间';
      return;
    }
    submitting = true;
    try {
      const data = await api.createOrder({
        ...form,
        base_fee_cents: Number(form.base_fee_cents),
        appointment_at: new Date(form.appointment_at).toISOString(),
      });
      createdId = data.order.id;
      form = {
        model: '',
        customer_name: '',
        customer_phone: '',
        appointment_at: '',
        base_fee_cents: 19800,
        scope_note: '',
      };
      await load();
      location.hash = `#/order/${createdId}`;
    } catch (e) {
      formError = e.message;
    } finally {
      submitting = false;
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

  onMount(load);
</script>

<div class="row">
  <div class="card" style="min-width:300px">
    <h2>客户预约</h2>
    <p class="hint">填写油烟机型号、联系人并预约上门时间；预约后可在订单页上传油烟机现场照片。</p>

    <label class="field">
      <span>油烟机型号 *</span>
      <input type="text" bind:value={form.model} placeholder="如：老板 CXW-260-67X2H" />
    </label>
    <div class="row">
      <label class="field">
        <span>联系人 *</span>
        <input type="text" bind:value={form.customer_name} placeholder="您的称呼" />
      </label>
      <label class="field">
        <span>手机号 *</span>
        <input type="tel" bind:value={form.customer_phone} placeholder="11 位手机号" />
      </label>
    </div>
    <div class="row">
      <label class="field">
        <span>预约上门时间 *</span>
        <input type="datetime-local" bind:value={form.appointment_at} />
      </label>
      <label class="field">
        <span>标准清洗套餐价（元）</span>
        <input
          type="number"
          min="0"
          step="1"
          value={form.base_fee_cents / 100}
          on:input={(e) => (form.base_fee_cents = Math.round(Number(e.target.value || 0) * 100))}
        />
      </label>
    </div>
    <label class="field">
      <span>施工范围备注</span>
      <textarea rows="2" bind:value={form.scope_note} placeholder="如：含风轮拆洗、油网清洁、外壳擦拭"></textarea>
    </label>

    {#if formError}
      <div class="alert error">{formError}</div>
    {/if}

    <button on:click={submit} disabled={submitting}>
      {submitting ? '提交中…' : '提交预约'}
    </button>
  </div>

  <div class="card" style="min-width:320px">
    <h2>订单列表</h2>
    <p class="hint">
      {role === 'customer' ? '客户可进入订单确认方案、核对完工结果并验收' : '师傅进入订单到场确认、登记零件与提交完工'}
    </p>

    {#if loading}
      <p class="muted">加载中…</p>
    {:else if loadError}
      <div class="alert error">{loadError}</div>
      <button class="ghost sm" on:click={load}>重试</button>
    {:else if orders.length === 0}
      <p class="muted">暂无订单，先在左侧提交一个预约吧。</p>
    {:else}
      <table class="list">
        <thead>
          <tr>
            <th>型号 / 客户</th>
            <th>预约时间</th>
            <th>金额</th>
            <th>状态</th>
            <th></th>
          </tr>
        </thead>
        <tbody>
          {#each orders as o (o.id)}
            <tr>
              <td>
                <strong>{o.model}</strong>
                <div class="muted small">{o.customer_name} · {o.customer_phone}</div>
              </td>
              <td class="small">{fmtTime(o.appointment_at)}</td>
              <td class="mono">{yuan(o.base_fee_cents)}</td>
              <td><span class="badge {statusClass(o.status)}">{STATUS_LABELS[o.status]}</span></td>
              <td>
                <a class="btn ghost sm" href={`#/order/${o.id}`}>进入</a>
              </td>
            </tr>
          {/each}
        </tbody>
      </table>
    {/if}
  </div>
</div>
