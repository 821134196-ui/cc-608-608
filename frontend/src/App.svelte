<script>
  import { onMount } from 'svelte'
  import Book from './views/Book.svelte'
  import Orders from './views/Orders.svelte'
  import OrderDetail from './views/OrderDetail.svelte'

  let route = { name: 'orders' }

  function parseHash() {
    const h = location.hash || '#/orders'
    const m = h.match(/^#\/order\/(\d+)/)
    if (m) route = { name: 'detail', id: Number(m[1]) }
    else if (h.startsWith('#/book')) route = { name: 'book' }
    else route = { name: 'orders' }
  }

  onMount(() => {
    parseHash()
    window.addEventListener('hashchange', parseHash)
    return () => window.removeEventListener('hashchange', parseHash)
  })
</script>

<header>
  <div class="wrap">
    <h1>🫧 油烟机清洗验收系统</h1>
    <nav>
      <a href="#/orders" class:active={route.name === 'orders'}>订单列表</a>
      <a href="#/book" class:active={route.name === 'book'}>客户预约</a>
    </nav>
  </div>
</header>

<main class="wrap">
  {#if route.name === 'book'}
    <Book />
  {:else if route.name === 'detail'}
    <OrderDetail id={route.id} />
  {:else}
    <Orders />
  {/if}
</main>

<style>
  :global(*) { box-sizing: border-box; }
  :global(body) {
    margin: 0;
    font-family: -apple-system, 'PingFang SC', 'Microsoft YaHei', 'Segoe UI', sans-serif;
    background: #f4f6f8;
    color: #24303f;
  }
  :global(.wrap) { max-width: 1080px; margin: 0 auto; padding: 0 16px; }
  :global(button) {
    font: inherit; cursor: pointer; border: none; border-radius: 8px;
    padding: 8px 16px; background: #2563eb; color: #fff;
  }
  :global(button:hover:not(:disabled)) { background: #1d4ed8; }
  :global(button:disabled) { background: #9ca3af; cursor: not-allowed; }
  :global(button.secondary) { background: #e5e7eb; color: #24303f; }
  :global(button.secondary:hover:not(:disabled)) { background: #d1d5db; }
  :global(button.danger) { background: #dc2626; }
  :global(button.success) { background: #16a34a; }
  :global(button.success:hover:not(:disabled)) { background: #15803d; }
  :global(input), :global(select), :global(textarea) {
    font: inherit; padding: 8px 10px; border: 1px solid #cbd5e1; border-radius: 8px; width: 100%;
  }
  :global(label) { display: block; font-size: 13px; color: #64748b; margin: 10px 0 4px; }
  :global(.card) {
    background: #fff; border-radius: 12px; padding: 18px 20px; margin-bottom: 16px;
    box-shadow: 0 1px 3px rgba(15, 23, 42, 0.08);
  }
  :global(.card h2) { margin: 0 0 12px; font-size: 17px; }
  :global(.badge) {
    display: inline-block; padding: 2px 10px; border-radius: 999px; font-size: 12px; font-weight: 600;
  }
  :global(.error-box) {
    background: #fef2f2; border: 1px solid #fecaca; color: #b91c1c;
    border-radius: 8px; padding: 10px 14px; margin: 10px 0; font-size: 14px;
  }
  :global(.ok-box) {
    background: #f0fdf4; border: 1px solid #bbf7d0; color: #15803d;
    border-radius: 8px; padding: 10px 14px; margin: 10px 0; font-size: 14px;
  }
  :global(table) { width: 100%; border-collapse: collapse; font-size: 14px; }
  :global(th), :global(td) { text-align: left; padding: 8px 10px; border-bottom: 1px solid #e2e8f0; }
  :global(th) { color: #64748b; font-weight: 600; }

  header {
    background: linear-gradient(135deg, #1e3a8a, #2563eb);
    color: #fff; padding: 14px 0; margin-bottom: 20px;
  }
  header .wrap { display: flex; align-items: center; justify-content: space-between; flex-wrap: wrap; gap: 8px; }
  header h1 { font-size: 19px; margin: 0; }
  nav a {
    color: #dbeafe; text-decoration: none; margin-left: 16px; padding: 6px 12px; border-radius: 8px;
  }
  nav a.active, nav a:hover { background: rgba(255, 255, 255, 0.18); color: #fff; }
</style>
