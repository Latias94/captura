(function(){
  function ensure(){
    let box = document.getElementById('toasts');
    if(!box){ box = document.createElement('div'); box.id='toasts'; box.className='toasts'; document.body.appendChild(box); }
    return box;
  }
  function showToast(msg, kind){
    const box = ensure();
    const el = document.createElement('div');
    el.className = 'toast' + (kind ? (' toast--' + kind) : '');
    el.textContent = msg;
    box.appendChild(el);
    setTimeout(()=>{ el.classList.add('toast--hide'); setTimeout(()=>{ el.remove(); }, 300); }, 2500);
  }
  window.showToast = showToast;
  // back-compat
  window.showAlert = showToast;
})();

