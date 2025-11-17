(function(){
  const view = document.getElementById('entriesView');
  const list = document.getElementById('savedSearches');
  const btn = document.getElementById('btnSaveSearch');
   const btnView = document.getElementById('btnSaveSmartView');
  const input = document.getElementById('searchInput');
  if(!view || !list || !input) return;
  const feedId = view.dataset.feedId;
  const KEY = 'captura:saved-searches';
  function load(){ try{ return JSON.parse(localStorage.getItem(KEY) || '{}') || {}; }catch(_){ return {}; } }
  function save(obj){ try{ localStorage.setItem(KEY, JSON.stringify(obj)); }catch(_){ } }
  function items(){ const o=load(); return o[feedId] || []; }
  function setItems(arr){ const o=load(); o[feedId]=arr; save(o); }
  function dict(name, fallback){ return (list.dataset[name]||fallback||''); }
  function redraw(){ list.innerHTML=''; const arr = items(); if(arr.length===0){ const li=document.createElement('li'); li.className='list__item'; li.textContent = dict('dictEmpty','No saved searches'); list.appendChild(li); return; } arr.forEach((it,idx)=>{ const li=document.createElement('li'); li.className='list__item'; const a=document.createElement('a'); a.className='link'; a.href = '/feeds/' + feedId + '?q=' + encodeURIComponent(it.q); a.textContent = it.name || it.q; li.appendChild(a); const ren=document.createElement('button'); ren.className='button ml-2'; ren.type='button'; ren.textContent=dict('dictRename','Rename'); ren.addEventListener('click', function(){ const nn = prompt(dict('dictRename','Rename'), it.name||it.q); if(nn){ const arr=items(); arr[idx].name=nn; setItems(arr); redraw(); }}); li.appendChild(ren); const del=document.createElement('button'); del.className='button ml-2'; del.type='button'; del.textContent=dict('dictDelete','Delete'); del.addEventListener('click', function(){ const arr=items(); arr.splice(idx,1); setItems(arr); redraw(); }); li.appendChild(del); list.appendChild(li); }); }
  if(btn){ btn.addEventListener('click', function(){ const q=(input.value||'').trim(); if(!q) return; const arr=items(); const exists = arr.find(x=>x.q===q); if(exists){ window.showToast && window.showToast(dict('dictExists','Already saved')); return; } const name = prompt(dict('dictSaveas','Save as'), q) || q; arr.unshift({name, q}); setItems(arr); redraw(); window.showToast && window.showToast(dict('dictSaved','Saved')); }); }
  redraw();

  function getToken(){
    const m = document.cookie.match(/(?:^|; )X-Auth-Token=([^;]+)/);
    return m ? decodeURIComponent(m[1]) : '';
  }

  async function saveSmartView(){
    const q = (input.value || '').trim();
    const filter = (view.dataset.filter || 'all').toLowerCase();
    if(!feedId) return;
    const token = getToken();
    if(!token) return;
    const keyName = 'dictSmartName';
    const namePrompt = dict(keyName, 'Save as view');
    const defaultName = q || namePrompt;
    const name = window.prompt(namePrompt, defaultName);
    if(!name) return;

    const filters = {};
    const fidNum = parseInt(feedId, 10);
    if(!Number.isNaN(fidNum)){
      filters.feed_ids = [fidNum];
    }
    if(q){ filters.search = q; }
    let status = null;
    if(filter === 'unread') status = 'unread';
    else if(filter === 'starred') status = 'starred';
    if(status){ filters.status = status; }

    const body = {
      name,
      view: 'all',
      filters,
      sort_by: 'published_at',
      sort_order: 'desc',
      pinned: true
    };
    try{
      const resp = await fetch('/api/v1/smart-views', {
        method: 'POST',
        headers: {
          'content-type': 'application/json',
          'Authorization': 'Bearer ' + token
        },
        body: JSON.stringify(body)
      });
      if(!resp.ok){
        window.showToast && window.showToast(dict('dictSmartFailed','Failed to save view'));
        return;
      }
      window.showToast && window.showToast(dict('dictSmartSaved','View saved'));
    }catch(_){
      window.showToast && window.showToast(dict('dictSmartFailed','Failed to save view'));
    }
  }

  if(btnView){
    btnView.addEventListener('click', function(){
      saveSmartView();
    });
  }
})();
