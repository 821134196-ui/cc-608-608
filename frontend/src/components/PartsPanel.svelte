<script>
  import { createEventDispatcher } from 'svelte';
  import { api, fmtTime } from '../api.js';

  export let orderId;
  export let parts = [];
  export let disabled = false;

  let name = '';
  let note = '';
  let busyName = '';
  let error = '';
  let adding = false;
  const dispatch = createEventDispatcher();

  $: missing = parts.filter((p) => !p.reinstalled_at);
  $: reinstalled = parts.length - missing.length;

  async function add() {
    error = '';
    if (!name.trim()) {
      error = '请填写零件名称';
      return;
    }
    adding = true;
    try {
      await api.addPart(orderId, name.trim(), note.trim());
      name = '';
      note = '';
      dispatch('changed');
    } catch (e) {
      error = e.message;
    } finally {
      adding = false;
    }
  }

  async function doReinstall(p) {
    busyName = p.id;
    error = '';
    try {
      await api.reinstall(orderId, p.id);
      dispatch('changed');
    } catch (e) {
      error = e.message;
    } finally {
      busyName = '';
    }
  }
</script>

<div>
  <div class="alert" class:ok={missing.length === 0 && parts.length > 0} class:warn={missing.length > 0}>
    {#if parts.length === 0}
      尚未登记拆下的零件。师傅需逐件登记，完工前全部复装。
    {:else if missing.length === 0}
      ✅ 全部 {parts.length} 个零件均已复装，复装清单完整。
    {:else}
      ⚠️ 已复装 {reinstalled}/{parts.length}，还有 {missing.length} 个零件未复装：
      {missing.map((m) => m.name).join('、')}。缺件状态下无法通过验收。
    {/if}
  </div>

  <ul class="parts">
    {#each parts as p (p.id)}
      <li>
        <span class="dot {p.reinstalled_at ? 'ok' : 'miss'}"></span>
        <strong style="flex:1">{p.name}</strong>
        {#if p.note}<span class="muted small">{p.note}</span>{/if}
        {#if p.reinstalled_at}
          <span class="badge green">已复装 · {fmtTime(p.reinstalled_at).slice(5)}</span>
        {:else}
          <span class="badge red">已拆下 · 待复装</span>
          {#if !disabled}
            <button class="sm ghost" disabled={busyName === p.id} on:click={() => doReinstall(p)}>
              {busyName === p.id ? '…' : '确认复装'}
            </button>
          {/if}
        {/if}
      </li>
    {/each}
  </ul>

  {#if !disabled}
    <h3>登记拆下零件</h3>
    <div class="row">
      <input type="text" bind:value={name} placeholder="零件名称，如：风轮 / 油网 / 油杯 / 出风罩" />
      <input type="text" bind:value={note} placeholder="备注（品牌/数量/外观）" />
    </div>
    <button class="sm" on:click={add} disabled={adding}>{adding ? '登记中…' : '＋ 登记拆下'}</button>
  {/if}

  {#if error}<div class="alert error">{error}</div>{/if}
</div>
