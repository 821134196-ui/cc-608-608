<script>
  import { createEventDispatcher } from 'svelte';
  import { api } from '../api.js';

  export let orderId;
  export let measurements = [];
  export let disabled = false;

  const dispatch = createEventDispatcher();
  let busy = '';
  let error = '';

  function find(phase) {
    return measurements.find((m) => m.phase === phase);
  }
  $: before = find('before');
  $: after = find('after');

  async function readAndSave(phase) {
    busy = phase;
    error = '';
    try {
      // 1) 从模拟测量仪读数（GET 实时读数接口）
      const r = await api.readMeter(orderId, phase);
      // 2) 记录入库（按 订单+阶段 upsert，重复提交不产生重复读数）
      await api.recordMeasurement(orderId, phase, r.reading.airflow_m3h, r.reading.note);
      dispatch('changed');
    } catch (e) {
      error = e.message;
    } finally {
      busy = '';
    }
  }
</script>

{#if error}<div class="alert error">{error}</div>{/if}

<div class="compare">
  <div class="col">
    <div class="col-head">
      <span>🔻 清洗前风量</span>
      <span class="badge gray">基准</span>
    </div>
    <div class="metric-big">
      {#if before}{before.airflow_m3h.toFixed(1)} <small>m³/h</small>{:else}<span class="muted">—</span>{/if}
    </div>
    <div class="muted small">{before ? `来源：${before.source}` : '尚未测量'}</div>
    {#if !disabled}
      <button class="sm ghost" style="margin-top:8px" disabled={busy === 'before'} on:click={() => readAndSave('before')}>
        {busy === 'before' ? '读取中…' : before ? '重新连接测量仪读数' : '连接测量仪读数'}
      </button>
    {/if}
  </div>

  <div class="col">
    <div class="col-head">
      <span>🔺 清洗后风量</span>
      {#if before && after && after.airflow_m3h > before.airflow_m3h}
        <span class="badge green">提升 {((after.airflow_m3h - before.airflow_m3h) / before.airflow_m3h * 100).toFixed(1)}%</span>
      {:else}
        <span class="badge gray">复测</span>
      {/if}
    </div>
    <div class="metric-big">
      {#if after}{after.airflow_m3h.toFixed(1)} <small>m³/h</small>{:else}<span class="muted">—</span>{/if}
    </div>
    <div class="muted small">{after ? `来源：${after.source}` : '尚未测量'}</div>
    {#if !disabled}
      <button class="sm ghost" style="margin-top:8px" disabled={busy === 'after'} on:click={() => readAndSave('after')}>
        {busy === 'after' ? '读取中…' : after ? '重新连接测量仪读数' : '连接测量仪读数'}
      </button>
    {/if}
  </div>
</div>
