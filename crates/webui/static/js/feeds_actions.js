(function(){
  function getToken(){
    const m = document.cookie.match(/(?:^|; )X-Auth-Token=([^;]+)/);
    return m ? decodeURIComponent(m[1]) : '';
  }
  function showAlert(text){
    const container = document.querySelector('.main') || document.body;
    const div = document.createElement('div');
    div.className = 'alert';
    div.textContent = text;
    container.insertBefore(div, container.firstChild);
    setTimeout(()=>{ div.remove(); }, 3000);
  }
  const btnAll = document.getElementById('btnRefreshAll');
  if(btnAll){
    btnAll.addEventListener('click', async function(){
      const token = getToken(); if(!token) return;
      try{
        const resp = await fetch('/v1/feeds/refresh', { method: 'PUT', headers: { 'X-Auth-Token': token }});
        if(resp && (resp.ok || resp.status === 204)){
          showAlert('Refresh enqueued');
          if(window.feedsCountersTick){ setTimeout(window.feedsCountersTick, 1500); }
        }
      }catch(_){ /* ignore */ }
    });
  }
  const formCat = document.getElementById('formRefreshCategory');
  if(formCat){
    formCat.addEventListener('submit', async function(e){
      e.preventDefault();
      const token = getToken(); if(!token) return;
      const id = (document.getElementById('refreshCategory')||{}).value || '';
      const q = id ? ('?category_id=' + encodeURIComponent(id)) : '';
      try{
        const resp = await fetch('/v1/feeds/refresh' + q, { method: 'PUT', headers: { 'X-Auth-Token': token }});
        if(resp && (resp.ok || resp.status === 204)){
          showAlert('Refresh enqueued');
          if(window.feedsCountersTick){ setTimeout(window.feedsCountersTick, 1500); }
        }
      }catch(_){ /* ignore */ }
    });
  }

  // View filter: navigate to /feeds or /feeds?category_id=ID
  const formView = document.getElementById('formViewCategory');
  if(formView){
    const sel = document.getElementById('viewCategory');
    if(sel){
      // restore remembered selection when no category_id in URL
      try{
        const params = new URLSearchParams(window.location.search);
        if(!params.get('category_id')){
          const saved = localStorage.getItem('captura:feeds:selected-category');
          if(saved && Array.from(sel.options).some(o => o.value === saved)){
            window.location.replace(saved ? ('/feeds?category_id=' + encodeURIComponent(saved)) : '/feeds');
          }
        }
      }catch(_){/* ignore */}
      sel.addEventListener('change', function(){
        const v = sel.value || '';
        try{ localStorage.setItem('captura:feeds:selected-category', v); }catch(_){ }
        const to = v ? ('/feeds?category_id=' + encodeURIComponent(v)) : '/feeds';
        window.location.href = to;
      });
    }
  }

  // Per-feed refresh: intercept list form submit
  const list = document.getElementById('feedsList');
  if(list){
    list.addEventListener('submit', async function(e){
      const form = e.target.closest('form');
      if(!form) return;
      const m = form.action.match(/\/ui\/feeds\/(\d+)\/refresh/);
      if(!m) return;
      const token = getToken(); if(!token) return; // progressive enhance only
      e.preventDefault();
      const id = m[1];
      try{
        const resp = await fetch(`/api/v1/feeds/${id}/refresh`, {
          method: 'POST',
          headers: { 'Authorization': `Bearer ${token}` }
        });
        if(resp && (resp.ok || resp.status === 204)){
          showAlert('Refresh requested');
          if(window.feedsCountersTick){ setTimeout(window.feedsCountersTick, 1500); }
        }
      }catch(_){ /* ignore */ }
    });
    // Category actions
    list.addEventListener('click', async function(e){
      const btn = e.target.closest('.cat-refresh, .cat-markall');
      if(!btn) return;
      const cat = btn.getAttribute('data-cat-id');
      if(!cat) return;
      const token = getToken(); if(!token) return;
      e.preventDefault();
      try{
        if(btn.classList.contains('cat-refresh')){
          const resp = await fetch(`/v1/categories/${cat}/refresh`, { method: 'PUT', headers: { 'X-Auth-Token': token }});
          if(resp && (resp.ok || resp.status === 204)){
            showAlert('Refresh enqueued');
            if(window.feedsCountersTick){ setTimeout(window.feedsCountersTick, 1500); }
          }
        }else if(btn.classList.contains('cat-markall')){
          const resp = await fetch('/api/v1/entries/mark-all-read', {
            method: 'POST',
            headers: {
              'Authorization': `Bearer ${token}`,
              'content-type': 'application/json'
            },
            body: JSON.stringify({ category_id: Number(cat) })
          });
          if(resp && (resp.ok || resp.status === 204)){
            showAlert('Marked category as read');
            if(window.feedsCountersTick){ setTimeout(window.feedsCountersTick, 1500); }
          }
        }
      }catch(_){/* ignore */}
    });
  }
})();
