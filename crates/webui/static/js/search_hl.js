(function(){
  const view = document.getElementById('entriesView');
  if(!view) return;
  const q = (view.dataset.search || '').trim();
  if(!q) return;
  // naive parse: split by spaces, handle #tag and author:xxx keep raw tokens
  const tokens = q.split(/\s+/).filter(Boolean);
  if(tokens.length === 0) return;
  function escRe(s){ return s.replace(/[.*+?^${}()|[\]\\]/g, '\\$&'); }
  function escHtml(s){ return s.replace(/[&<>"']/g, c => ({'&':'&amp;','<':'&lt;','>':'&gt;','"':'&quot;',"'":'&#39;'}[c])); }
  function highlightEl(el){
    const txt = el.textContent || '';
    if(!txt) return;
    let html = escHtml(txt);
    const tagTokens = tokens.filter(t => /^#|^tag:/i.test(t)).map(t => t.replace(/^tag:/i,'').replace(/^#/,'')).filter(Boolean);
    const authorTokens = tokens.filter(t => /^author:/i.test(t)).map(t => t.replace(/^author:/i,'')).filter(Boolean);
    const normalTokens = tokens.filter(t => !/^#|^tag:|^author:/i.test(t));
    tagTokens.forEach(term => { const re = new RegExp('('+escRe(escHtml(term))+')','ig'); html = html.replace(re, '<span class="badge badge--accent">$1</span>'); });
    authorTokens.forEach(term => { const re = new RegExp('('+escRe(escHtml(term))+')','ig'); html = html.replace(re, '<span class="badge badge--accent">$1</span>'); });
    normalTokens.forEach(term => { const re = new RegExp('('+escRe(escHtml(term))+')','ig'); html = html.replace(re, '<mark>$1</mark>'); });
    el.innerHTML = html;
  }
  // highlight titles and meta spans
  const titles = document.querySelectorAll('#cards .card__title');
  titles.forEach(a => highlightEl(a));
  const metas = document.querySelectorAll('#cards .card__meta span');
  metas.forEach(span => highlightEl(span));
})();
