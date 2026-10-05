<script>
  import { createEventDispatcher } from 'svelte';
  import { api, fmtTime } from '../api.js';

  export let orderId;
  export let photos = [];
  export let disabled = false;
  export let phase; // 'before' | 'after'

  let file;
  let label = '';
  let busy = false;
  let error = '';
  const dispatch = createEventDispatcher();

  function onPick(e) {
    file = e.target.files[0] || null;
  }

  async function upload() {
    error = '';
    if (!file) {
      error = '请先选择照片';
      return;
    }
    busy = true;
    try {
      await api.uploadPhoto(orderId, file, phase, label);
      file = null;
      label = '';
      dispatch('changed');
    } catch (e) {
      error = e.message;
    } finally {
      busy = false;
    }
  }

  $: shown = photos.filter((p) => p.phase === phase);
</script>

<div>
  <div class="photo-grid">
    {#each shown as p (p.id)}
      <figure>
        <img class="photo-thumb" src={`/uploads/${p.rel_path}`} alt={p.label || phase} />
        <figcaption>{p.label || '现场照片'} · {fmtTime(p.created_at).slice(5)}</figcaption>
      </figure>
    {:else}
      <p class="muted small">暂无{phase === 'before' ? '清洗前' : '清洗后'}照片</p>
    {/each}
  </div>

  {#if !disabled}
    <div style="margin-top:10px">
      <input type="file" accept="image/jpeg,image/png,image/webp" on:change={onPick} />
      <div class="row" style="margin-top:8px">
        <input
          type="text"
          bind:value={label}
          placeholder="照片备注（如：油网、风轮）"
          style="flex:2"
        />
        <button class="sm" on:click={upload} disabled={busy} style="flex:none">
          {busy ? '上传中…' : '上传照片'}
        </button>
      </div>
      {#if error}<div class="alert error">{error}</div>{/if}
    </div>
  {/if}
</div>
