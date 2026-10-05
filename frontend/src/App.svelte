<script>
  import { onMount } from 'svelte';
  import Home from './pages/Home.svelte';
  import Order from './pages/Order.svelte';

  // 演示系统用“身份切换”区分 客户 / 师傅 两侧操作；
  // 真实部署中由登录态决定，这里存在 localStorage。
  let role = 'technician';
  let route = { name: 'home' };

  function parseHash() {
    const hash = location.hash.replace(/^#/, '') || '/';
    const m = hash.match(/^\/order\/([A-Za-z0-9]+)/);
    if (m) route = { name: 'order', id: m[1] };
    else route = { name: 'home' };
  }

  function setRole(r) {
    role = r;
    localStorage.setItem('hoodcare_role', r);
  }

  onMount(() => {
    role = localStorage.getItem('hoodcare_role') || 'technician';
    parseHash();
    window.addEventListener('hashchange', parseHash);
    return () => window.removeEventListener('hashchange', parseHash);
  });
</script>

<div class="topbar">
  <div class="topbar-inner">
    <span class="logo">🍳 净烟工坊</span>
    <span class="sub">油烟机清洗 · 前后验收系统</span>
    <span class="spacer"></span>
    <div class="seg" role="group" aria-label="身份切换">
      <button class={role === 'customer' ? 'active' : ''} on:click={() => setRole('customer')}>
        客户视图
      </button>
      <button class={role === 'technician' ? 'active' : ''} on:click={() => setRole('technician')}>
        师傅视图
      </button>
    </div>
  </div>
</div>

<div class="container">
  {#if route.name === 'home'}
    <Home {role} />
  {:else if route.name === 'order'}
    <Order {role} orderId={route.id} />
  {/if}
</div>

<style>
  .seg button {
    background: rgba(255, 255, 255, 0.14);
    color: #fff;
    border: 1px solid rgba(255, 255, 255, 0.35);
  }
  .seg button.active {
    background: #fff;
    color: var(--brand-dark);
  }
</style>
