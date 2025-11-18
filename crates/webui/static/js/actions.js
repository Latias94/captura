// Progressive enhancement: use fetch to update entry state without full reload.
(function(){
  function getToken(){
    const m = document.cookie.match(/(?:^|; )X-Auth-Token=([^;]+)/);
    return m ? decodeURIComponent(m[1]) : '';
  }

  function showAlert(text){ if(window.showToast){ window.showToast(text); } else { const container = document.querySelector('.main') || document.body; const div = document.createElement('div'); div.className = 'alert'; div.textContent = text; container.insertBefore(div, container.firstChild); setTimeout(()=>{ div.remove(); }, 3000); } }

  function isStarred(btn){
    if(!btn) return false;
    const text = (btn.textContent || '').trim();
    const starLabel = btn.getAttribute('data-label-star') || 'Star';
    const unstarLabel = btn.getAttribute('data-label-unstar') || 'Unstar';
    if(text === '★') return true;
    if(text === '☆') return false;
    if(text === unstarLabel) return true;
    if(text === starLabel) return false;
    return false;
  }

  async function toggleStar(entryId, btn){
    const token = getToken();
    if(!token) return false;
    const next = !isStarred(btn);
    try{
      const resp = await fetch(`/api/v1/entries/${entryId}/star`, {
        method: 'POST',
        headers: {
          'Authorization': `Bearer ${token}`,
          'content-type': 'application/json'
        },
        body: JSON.stringify({ value: next })
      });
      if(!resp.ok) return false;
      // entries list uses glyph ★/☆; entry page uses localized labels
      if(btn){
        const starLabel = btn.getAttribute('data-label-star') || 'Star';
        const unstarLabel = btn.getAttribute('data-label-unstar') || 'Unstar';
        if(btn.textContent.trim() === '★' || btn.textContent.trim() === '☆'){
          btn.textContent = next ? '★' : '☆';
        } else {
          btn.textContent = next ? unstarLabel : starLabel;
        }
      }
      return true;
    }catch(_){ return false; }
  }

  async function setReadStatus(entryId, status, btn, root){
    const token = getToken();
    if(!token) return false;
    try{
      const resp = await fetch(`/api/v1/entries/${entryId}/read`, {
        method: 'POST',
        headers: {
          'Authorization': `Bearer ${token}`,
          'content-type': 'application/json'
        },
        body: JSON.stringify({ value: status === 'read' })
      });
      if(!resp.ok) return false;
      if(root){ root.dataset.status = status; }
      if(btn){
        const readLabel = btn.getAttribute('data-label-read') || 'Mark read';
        const unreadLabel = btn.getAttribute('data-label-unread') || 'Mark unread';
        btn.textContent = (status === 'read') ? unreadLabel : readLabel;
      }
      return true;
    }catch(_){ return false; }
  }

  // Intercept star/unread forms on entry page
  const root = document.getElementById('entryRoot');
  if(root){
    const id = root.dataset.entryId;
    const starForm = document.getElementById('btnStar')?.closest('form');
    const markForm = document.getElementById('btnMark')?.closest('form');
    const btnKeep = document.getElementById('btnKeepUnread');
    const btnSave = document.getElementById('btnSaveEntry');
    const btnFull = document.getElementById('btnLoadFull');
    if(starForm){
      starForm.addEventListener('submit', function(e){
        if(!getToken()) return; // fallback to normal submit
        e.preventDefault();
        toggleStar(id, document.getElementById('btnStar'));
      });
    }
    if(markForm){
      markForm.addEventListener('submit', function(e){
        if(!getToken()) return; // fallback to normal submit
        e.preventDefault();
        const current = (root.dataset.status || '').toLowerCase();
        const next = current === 'read' ? 'unread' : 'read';
        setReadStatus(id, next, document.getElementById('btnMark'), root);
      });
    }
    // auto mark read on open
    if(/(?:^|; )auto_mark_read=1/.test(document.cookie)){
      const current = (root.dataset.status||'').toLowerCase();
      if(current !== 'read'){
        setReadStatus(id, 'read', document.getElementById('btnMark'), root);
      }
    }
    if(btnKeep){
      btnKeep.addEventListener('click', function(){ if(!getToken()) return; setReadStatus(id, 'unread', document.getElementById('btnMark'), root); });
    }
    async function saveEntry(entryId){
      const token = getToken(); if(!token) return false;
      try{
        const r = await fetch(`/api/v1/entries/${entryId}/save`, {
          method: 'POST',
          headers: {
            'Authorization': `Bearer ${token}`,
            'content-type': 'application/json'
          },
          body: JSON.stringify({ value: true })
        });
        return r.ok;
      }catch(_){ return false; }
    }
    if(btnSave){ btnSave.addEventListener('click', async function(){ if(!getToken()) return; const ok = await saveEntry(id); if(ok) showAlert('Saved entry'); }); }
    async function loadFull(entryId, persist){
      const token = getToken(); if(!token) return null;
      const url = `/api/v1/entries/${entryId}/content` + (persist ? '?update_content=true' : '');
      try{
        const r = await fetch(url, { headers: { 'Authorization': `Bearer ${token}` }});
        if(!r.ok) return null;
        return await r.json();
      }catch(_){ return null; }
    }
    if(btnFull){ btnFull.addEventListener('click', async function(){ const data = await loadFull(id, true); if(data && data.content_html){ const el = document.querySelector('.article__content'); if(el){ el.innerHTML = data.content_html; showAlert('Loaded full content'); } }}); }
    // tags add/remove
    const tagsBox = document.getElementById('tagsList');
    const tagsInput = document.getElementById('tagsAdd');
    const tagsBtn = document.getElementById('tagsAddBtn');
    function renderTags(tags){ if(!tagsBox) return; tagsBox.innerHTML = (tags||[]).map(t => `<span class="tag" data-tag="${t}">${t} <button class="tag__remove" type="button" data-tag="${t}">×</button></span>`).join(' '); }
    async function addTags(entryId, tags){
      const token=getToken(); if(!token) return false;
      try{
        const r= await fetch(`/api/v1/entries/${entryId}/tags`, {
          method:'POST',
          headers:{
            'Authorization': `Bearer ${token}`,
            'content-type':'application/json'
          },
          body: JSON.stringify({tags})
        });
        return r.ok;
      }catch(_){ return false; }
    }
    async function removeTags(entryId, tags){
      const token=getToken(); if(!token) return false;
      try{
        const r= await fetch(`/api/v1/entries/${entryId}/tags`, {
          method:'DELETE',
          headers:{
            'Authorization': `Bearer ${token}`,
            'content-type':'application/json'
          },
          body: JSON.stringify({tags})
        });
        return r.ok;
      }catch(_){ return false; }
    }
    if(tagsBtn && tagsInput){ tagsBtn.addEventListener('click', async function(){ const raw=(tagsInput.value||'').trim(); if(!raw) return; const tags=raw.split(',').map(s=>s.trim()).filter(Boolean); const ok= await addTags(id, tags); if(ok){ // fetch entry to update tags list
          // simply append without refetch
          const current = Array.from((tagsBox||{}).querySelectorAll('[data-tag]')||[]).map(el=>el.getAttribute('data-tag'));
          const merged = Array.from(new Set([...(current||[]), ...tags]));
          renderTags(merged);
          tagsInput.value='';
        }
      }); }
    if(tagsBox){ tagsBox.addEventListener('click', async function(e){ const b = e.target.closest('.tag__remove'); if(!b) return; const tg = b.getAttribute('data-tag'); if(!tg) return; const ok = await removeTags(id, [tg]); if(ok){ b.closest('.tag').remove(); } }); }
  }

  // Intercept star forms on entries list
  const list = document.getElementById('cards');
  if(list){
    list.addEventListener('submit', function(e){
      const form = e.target.closest('form');
      if(!form) return;
      const m = form.action.match(/\/ui\/entries\/(\d+)\/toggle-star/);
      if(m){
        if(!getToken()) return; // fallback to normal submit
        e.preventDefault();
        const id = m[1];
        const btn = form.querySelector('button');
        toggleStar(id, btn);
      }
    });
    // Bulk mark via API without reload
    const view = document.getElementById('entriesView');
    function getFilter(){ return (view && view.dataset.filter) ? view.dataset.filter.toLowerCase() : 'all'; }
    function getLimit(){ return (view && view.dataset.limit) ? parseInt(view.dataset.limit, 10) || 50 : 50; }
    function getFeedId(){ return (view && view.dataset.feedId) ? view.dataset.feedId : null; }
    function tPick(){ return (view && view.dataset.pickLabel) ? view.dataset.pickLabel : 'Pick'; }
    function tNoTitle(){ return (view && view.dataset.noTitle) ? view.dataset.noTitle : 'No title'; }
    function picks(){ return Array.from(list.querySelectorAll('.card__pick')); }
    function selectedIds(){ return picks().filter(cb => cb.checked).map(cb => cb.dataset.id); }
  async function bulkMark(ids, status){
      const token = getToken();
      if(!token || ids.length === 0) return false;
      const resp = await fetch('/api/v1/entries/bulk-status', {
        method: 'POST',
        headers: {
          'Authorization': `Bearer ${token}`,
          'content-type': 'application/json'
        },
        body: JSON.stringify({ entry_ids: ids.map(Number), status })
      });
      if(!resp.ok) return false;
      // Update DOM
      ids.forEach(id => {
        const li = list.querySelector(`.card[data-entry-id="${id}"]`);
        if(li){
          li.dataset.status = status;
          if(status === 'read' && getFilter() === 'unread'){
            // Hide in unread filter to simulate removal
            li.classList.add('hidden');
          }
        }
      });
      return true;
    }

    const formRead = document.getElementById('formMarkRead');
    const formUnread = document.getElementById('formMarkUnread');
    const formPage = document.getElementById('formMarkPageRead');
    const formAbove = document.getElementById('formMarkAboveRead');
    const formBelow = document.getElementById('formMarkBelowRead');
    async function updateCounterBadge(){
      const fid = getFeedId();
      if(!fid) return;
      try{
        const resp = await fetch('/api/v1/feeds/counters', {
          headers: { 'Authorization': `Bearer ${getToken()}` }
        });
        if(!resp.ok) return;
        const json = await resp.json();
        const n = (json && json.unreads) ? (json.unreads[String(fid)] || 0) : 0;
        const el = document.getElementById('feedUnread');
        if(el){ el.textContent = n; }
        // also update nav total if present
        const nav = document.getElementById('navUnread');
        if(nav && json && json.unreads){
          let total = 0; for(const k in json.unreads){ if(Object.prototype.hasOwnProperty.call(json.unreads,k)) total += json.unreads[k] || 0; }
          nav.textContent = total;
          if(total > 0) nav.removeAttribute('hidden'); else nav.setAttribute('hidden','');
        }
      }catch(_){ /* ignore */ }
    }

    if(formRead){ formRead.addEventListener('submit', async function(e){ if(!getToken()) return; e.preventDefault(); const ok = await bulkMark(selectedIds(), 'read'); if(ok){ showAlert('Marked selected as read'); updateCounterBadge(); } }); }
    if(formUnread){ formUnread.addEventListener('submit', async function(e){ if(!getToken()) return; e.preventDefault(); const ok = await bulkMark(selectedIds(), 'unread'); if(ok){ showAlert('Marked selected as unread'); updateCounterBadge(); } }); }
    if(formPage){ formPage.addEventListener('submit', async function(e){ if(!getToken()) return; e.preventDefault(); const all = picks().map(cb => cb.dataset.id); const ok = await bulkMark(all, 'read'); if(ok){ showAlert('Marked page as read'); updateCounterBadge(); } }); }
    if(formAbove){ formAbove.addEventListener('submit', async function(e){ if(!getToken()) return; e.preventDefault(); const cards = Array.from(list.querySelectorAll('.card:not(.hidden)')); const active = list.querySelector('.card--active'); if(!active){ return; } const idx = cards.indexOf(active); if(idx <= 0){ return; } const ids = cards.slice(0, idx).map(li => li.dataset.entryId); const ok = await bulkMark(ids, 'read'); if(ok){ showAlert('Marked above as read'); updateCounterBadge(); } }); }
    if(formBelow){ formBelow.addEventListener('submit', async function(e){ if(!getToken()) return; e.preventDefault(); const cards = Array.from(list.querySelectorAll('.card:not(.hidden)')); const active = list.querySelector('.card--active'); if(!active){ return; } const idx = cards.indexOf(active); if(idx < 0 || idx >= cards.length-1){ return; } const ids = cards.slice(idx+1).map(li => li.dataset.entryId); const ok = await bulkMark(ids, 'read'); if(ok){ showAlert('Marked below as read'); updateCounterBadge(); } }); }

    // Feed toolbar: mark all read / refresh
    const viewEl = document.getElementById('entriesView');
    const feedId = viewEl ? viewEl.dataset.feedId : null;
    const formFeedAll = document.getElementById('formFeedMarkAllRead');
    const formRefresh = document.getElementById('formFeedRefresh');
    if(formFeedAll && feedId){
      formFeedAll.addEventListener('submit', async function(e){
        if(!getToken()) return; e.preventDefault();
        try{
          const resp = await fetch('/api/v1/entries/mark-all-read', {
            method: 'POST',
            headers: {
              'Authorization': `Bearer ${getToken()}`,
              'content-type': 'application/json'
            },
            body: JSON.stringify({ feed_id: Number(feedId) })
          });
          if(resp.ok){
            // Hide all cards in unread filter
            if(getFilter() === 'unread'){
              Array.from(list.querySelectorAll('.card')).forEach(li => li.classList.add('hidden'));
            } else {
              Array.from(list.querySelectorAll('.card')).forEach(li => li.dataset.status = 'read');
            }
            showAlert('Marked feed as read');
            updateCounterBadge();
          }
        }catch(_){ /* ignore */ }
      });
    }
    if(formRefresh && feedId){
      formRefresh.addEventListener('submit', async function(e){
        if(!getToken()) return; e.preventDefault();
        try{
          const resp = await fetch(`/api/v1/feeds/${feedId}/refresh`, {
            method: 'POST',
            headers: { 'Authorization': `Bearer ${getToken()}` }
          });
          if(resp.ok){
            showAlert('Refresh requested');
            // Poll for new entries up to 3 times
            const topId = (list.querySelector('.card') || {}).dataset?.entryId;
            let tries = 3;
            while(tries-- > 0){
              await new Promise(r => setTimeout(r, 2000));
              const data = await fetchUiEntries(getFeedId(), getLimit(), getFilter());
              if(data && data.entries && data.entries.length){
                if(!topId || String(data.entries[0].id) !== String(topId)){
                  mergeList(data.entries);
                  showAlert('Entries updated');
                  updateCounterBadge();
                  break;
                }
              }
            }
          }
        }catch(_){ /* ignore */ }
      });
    }

    async function fetchUiEntries(feedId, limit, filter){
      if(!feedId) return null;
      const token = getToken();
      const p = new URLSearchParams();
      p.set('limit', String(limit || 50));
      p.set('offset', '0');
      p.set('order', 'published_at');
      p.set('direction', 'desc');
      if(filter === 'unread') p.set('status', 'unread');
      if(filter === 'starred') p.set('starred', 'true');
      const url = `/v1/feeds/${feedId}/entries?` + p.toString();
      try{
        const resp = await fetch(url, { headers: { 'X-Auth-Token': token }});
        if(!resp.ok) return null;
        return await resp.json();
      }catch(_){ return null; }
    }

    function escapeHtml(s){ return (s||'').replace(/[&<>"']/g, c => ({'&':'&amp;','<':'&lt;','>':'&gt;','"':'&quot;',"'":'&#39;'}[c])); }
    function rebuildList(entries){
      const html = entries.map(e => {
        const id = e.id;
        const title = e.title ? escapeHtml(e.title) : tNoTitle();
        const author = e.author ? `<span>${escapeHtml(e.author)}</span>` : '';
        const date = e.date ? `<span>${escapeHtml(e.date)}</span>` : '';
        const starred = e.starred === true;
        const starGlyph = starred ? '★' : '☆';
        const status = e.status || (e.is_read ? 'read' : 'unread');
        return `
          <li class="card" tabindex="0" data-entry-id="${id}" data-status="${status}">
            <label class="float-right"><input type="checkbox" class="card__pick" data-id="${id}" /> ${escapeHtml(tPick())}</label>
            <a class="card__title" href="/entries/${id}">${title}</a>
            <div class="card__meta">
              ${author}
              ${date}
              <form method="post" action="/ui/entries/${id}/toggle-star" class="inline">
                <button type="submit" class="button button--xs">${starGlyph}</button>
              </form>
            </div>
          </li>`;
      }).join('');
      list.innerHTML = html;
    }

    function mergeList(entries){
      // Build a set of existing IDs
      const existing = new Set(Array.from(list.querySelectorAll('.card')).map(li => li.dataset.entryId));
      const fr = document.createDocumentFragment();
      let inserted = 0;
      entries.forEach(e => {
        const id = String(e.id);
        if(existing.has(id)) return;
        const li = document.createElement('li');
        li.className = 'card';
        li.setAttribute('tabindex','0');
        li.dataset.entryId = id;
        li.dataset.status = e.status || (e.is_read ? 'read' : 'unread');
        const title = e.title ? escapeHtml(e.title) : tNoTitle();
        const author = e.author ? `<span>${escapeHtml(e.author)}</span>` : '';
        const date = e.date ? `<span>${escapeHtml(e.date)}</span>` : '';
        const starred = e.starred === true;
        const starGlyph = starred ? '★' : '☆';
        li.innerHTML = `
            <label class="float-right"><input type="checkbox" class="card__pick" data-id="${id}" /> ${escapeHtml(tPick())}</label>
            <a class="card__title" href="/entries/${id}">${title}</a>
            <div class="card__meta">
              ${author}
              ${date}
              <form method="post" action="/ui/entries/${id}/toggle-star" class="inline">
                <button type="submit" class="button button--xs">${starGlyph}</button>
              </form>
            </div>`;
        fr.appendChild(li);
        inserted++;
      });
      if(inserted > 0){
        const first = list.firstChild;
        list.insertBefore(fr, first);
      }
    }
  }
})();
    // click author to append author: token to search
    list.addEventListener('click', function(e){
      const el = e.target.closest('.meta-author');
      if(!el) return;
      const name = el.getAttribute('data-author') || el.textContent || '';
      const form = document.querySelector('form[action^="/feeds/"][method="get"]');
      const input = document.getElementById('searchInput');
      if(form && input && name){
        const cur = (input.value||'').trim();
        const token = 'author:"' + name.replace(/"/g,'\\"') + '"';
        input.value = cur ? (cur + ' ' + token) : token;
        form.requestSubmit();
      }
    });
    // click tag to append #tag to search
    list.addEventListener('click', function(e){
      const el = e.target.closest('.entry-tag');
      if(!el) return;
      const tag = el.getAttribute('data-tag') || el.textContent.replace(/^#/,'') || '';
      const form = document.querySelector('form[action^="/feeds/"][method="get"]');
      const input = document.getElementById('searchInput');
      if(form && input && tag){
        const cur = (input.value||'').trim();
        const token = '#' + tag;
        input.value = cur ? (cur + ' ' + token) : token;
        form.requestSubmit();
      }
    });
