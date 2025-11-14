// Bulk select on entries list: select all, mark read/unread
(function(){
  const list = document.getElementById('cards');
  if(!list) return;
  const btnAll = document.getElementById('btnSelectAll');
  const picks = () => Array.from(list.querySelectorAll('.card__pick'));
  function selectedIds(){ return picks().filter(cb => cb.checked).map(cb => cb.dataset.id).join(','); }
  if(btnAll){
    btnAll.addEventListener('click', function(){
      const all = picks();
      const anyUnchecked = all.some(cb => !cb.checked);
      all.forEach(cb => cb.checked = anyUnchecked);
    });
  }
  const formRead = document.getElementById('formMarkRead');
  const formUnread = document.getElementById('formMarkUnread');
  const formPage = document.getElementById('formMarkPageRead');
  const btnPage = document.getElementById('btnMarkPageRead');
  if(formRead){
    formRead.addEventListener('submit', function(e){
      const ids = document.getElementById('idsRead');
      ids.value = selectedIds();
      if(!ids.value){ e.preventDefault(); }
    });
  }
  if(formUnread){
    formUnread.addEventListener('submit', function(e){
      const ids = document.getElementById('idsUnread');
      ids.value = selectedIds();
      if(!ids.value){ e.preventDefault(); }
    });
  }
  if(formPage && btnPage){
    formPage.addEventListener('submit', function(e){
      const ids = document.getElementById('idsPageRead');
      // collect all entries on page
      const all = picks();
      ids.value = all.map(cb => cb.dataset.id).join(',');
      if(!ids.value){ e.preventDefault(); }
    });
  }
})();
