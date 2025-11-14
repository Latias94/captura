(function(){
  const file = document.getElementById('opmlFile');
  const btn = document.getElementById('opmlFileBtn');
  if(!file || !btn) return;
  btn.addEventListener('click', function(){
    const f = file.files && file.files[0];
    if(!f) return;
    const r = new FileReader();
    r.onload = function(){
      const content = r.result || '';
      const form = document.createElement('form');
      form.method = 'post';
      form.action = '/ui/opml/import';
      const ta = document.createElement('textarea');
      ta.name = 'content'; ta.value = content;
      form.appendChild(ta);
      document.body.appendChild(form);
      form.submit();
    };
    r.readAsText(f);
  });
})();

