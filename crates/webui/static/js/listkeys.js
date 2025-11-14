// Minimal j/k navigation for entries list
(function(){
  const list = document.getElementById('cards');
  if(!list) return;
  let idx = -1;
  function items(){ return Array.from(list.querySelectorAll('.card:not(.hidden)')); }
  function focusAt(i){
    const arr = items();
    if(i<0 || i>=arr.length) return;
    if(idx>=0 && arr[idx]) arr[idx].classList.remove('card--active');
    idx = i; arr[idx].classList.add('card--active'); arr[idx].focus();
  }
  window.addEventListener('keydown', function(e){
    const tag = (e.target && e.target.tagName || '').toLowerCase();
    if (tag === 'input' || tag === 'textarea') return;
    const arr = items();
    if (e.key === 'j' || e.key === 'J') { focusAt(Math.min(idx+1, arr.length-1)); e.preventDefault(); }
    else if (e.key === 'k' || e.key === 'K') { focusAt(Math.max(idx-1, 0)); e.preventDefault(); }
    else if (e.key === 'o' || e.key === 'O' || e.key === 'Enter') { if(idx>=0){ const a = arr[idx].querySelector('a.card__title'); if(a){ a.click(); e.preventDefault(); }}}
    else if (e.key === 'x' || e.key === 'X') { if(idx>=0){ const cb = arr[idx].querySelector('.card__pick'); if(cb){ cb.checked = !cb.checked; e.preventDefault(); }}}
    else if ((e.key === 'a' || e.key === 'A') && !e.shiftKey) { const boxes = Array.from(list.querySelectorAll('.card__pick')); const anyUnchecked = boxes.some(cb => !cb.checked); boxes.forEach(cb => cb.checked = anyUnchecked); e.preventDefault(); }
    else if ((e.key === 'A' || e.key === 'a') && e.shiftKey) { const form = document.getElementById('formMarkPageRead'); if(form){ form.requestSubmit(); e.preventDefault(); } }
    else if ((e.key === 'J') && e.shiftKey) { const form = document.getElementById('formMarkBelowRead'); if(form){ form.requestSubmit(); e.preventDefault(); } }
    else if ((e.key === 'K') && e.shiftKey) { const form = document.getElementById('formMarkAboveRead'); if(form){ form.requestSubmit(); e.preventDefault(); } }
    else if (e.key === '/') { const inp = document.getElementById('searchInput'); if(inp){ inp.focus(); e.preventDefault(); } }
    else if (e.key === 'Escape') { const inp = document.getElementById('searchInput'); if(inp && document.activeElement === inp){ const feed = document.getElementById('entriesView')?.dataset.feedId; const limit = document.getElementById('entriesView')?.dataset.limit; const filter = (document.getElementById('entriesView')?.dataset.filter||'all'); let url = '/feeds/' + feed + '?limit=' + (limit||''); if(filter === 'unread'){ url += '&status=unread'; } else if(filter === 'starred'){ url += '&starred=true'; } window.location.href = url; e.preventDefault(); } }
  }, {passive:false});
})();
