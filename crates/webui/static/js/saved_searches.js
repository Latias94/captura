(function(){
  const view = document.getElementById('entriesView');
  const list = document.getElementById('savedSearches');
  const btn = document.getElementById('btnSaveSearch');
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
})();
