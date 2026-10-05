<script>
  import { api } from '../api.js'

  let form = {
    customer_name: '',
    phone: '',
    address: '',
    hood_model: '',
    appointment_time: '',
    base_price_cents: 20000,
  }
  let files = []
  let error = ''
  let submitting = false

  async function submit() {
    error = ''
    submitting = true
    try {
      const order = await api.createOrder({
        ...form,
        base_price_cents: Math.round(Number(form.base_price_cents) || 0),
      })
      for (const f of files) {
        await api.uploadPhoto(order.id, 'booking', f)
      }
      location.hash = `#/order/${order.id}`
    } catch (e) {
      error = e.message
    } finally {
      submitting = false
    }
  }

  function onFiles(e) {
    files = Array.from(e.target.files || [])
  }
</script>

<div class="card" style="max-width: 560px; margin: 0 auto;">
  <h2>客户预约</h2>
  <p style="color:#64748b; font-size:14px; margin-top:0;">
    填写油烟机信息并上传机器照片，门店将按预约时间上门服务。
  </p>

  <label>客户姓名</label>
  <input bind:value={form.customer_name} placeholder="张女士" />

  <label>联系电话</label>
  <input bind:value={form.phone} placeholder="138xxxxxxxx" />

  <label>服务地址</label>
  <input bind:value={form.address} placeholder="幸福小区 1 栋 101" />

  <label>油烟机型号</label>
  <input bind:value={form.hood_model} placeholder="如：方太 CXW-200-EMC5" />

  <label>预约上门时间</label>
  <input bind:value={form.appointment_time} placeholder="2026-10-08 14:00" />

  <label>预估基础价格（分）</label>
  <input type="number" bind:value={form.base_price_cents} min="0" step="100" />

  <label>油烟机照片（可多选）</label>
  <input type="file" accept="image/*" multiple on:change={onFiles} />
  {#if files.length}
    <p style="font-size:13px; color:#64748b;">已选择 {files.length} 张照片</p>
  {/if}

  {#if error}<div class="error-box">{error}</div>{/if}

  <div style="margin-top:16px;">
    <button on:click={submit} disabled={submitting}>
      {submitting ? '提交中…' : '提交预约'}
    </button>
  </div>
</div>
