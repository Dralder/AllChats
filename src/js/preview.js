(function () {
  if (window.__acPreview) return;
  window.__acPreview = true;
  var U = window.__acU;
  var hidden = __HIDDEN__;
  var bgs = __BGS__;
  var fade = __FADE__;
  var opacity = __OPACITY__;
  var style = document.createElement('style');
  var box = document.createElement('div');
  box.style.cssText = 'position:fixed;z-index:2147483646;pointer-events:none;border:2px solid #22D3EE;background:rgba(34,211,238,.12);display:none;box-sizing:border-box';
  var lastEvent = null;
  var shiftDown = false;

  function render() {
    var css = hidden.map(function (i) {
      return i.sel + '{opacity:.15!important;outline:2px solid #B83232!important;outline-offset:-2px!important}';
    }).join('\n');
    css += '\n' + fade.map(function (i) { return i.sel + '{outline:2px solid #D4A94A!important;outline-offset:-2px!important}'; }).join('\n');
    css += '\n' + U.bgCss(bgs);
    css += '\n' + U.baseCss(opacity);
    style.textContent = css;
    U.applyBgDirect(opacity);
  }

  function mount() {
    var root = document.documentElement;
    if (!root) return false;
    if (!style.parentNode) root.appendChild(style);
    if (!box.parentNode) root.appendChild(box);
    return true;
  }

  function skip(el) {
    if (!el || el === document.documentElement || el === document.body || el === box) return true;
    return false;
  }

  function matchIn(list, el) {
    for (var i = 0; i < list.length; i++) {
      try { if (el.matches(list[i].sel)) return i; } catch (e) {}
    }
    return -1;
  }

  function resolve(e) {
    var t = e.target;
    if (t && t.nodeType !== 1) t = t.parentElement;
    if (skip(t)) return null;
    var useShift = e.shiftKey || shiftDown;
    if (useShift && t) {
      var start = t;
      var startRect = start.getBoundingClientRect();
      var parent = start.parentElement;
      while (parent && !skip(parent)) {
        var parentRect = parent.getBoundingClientRect();
        if (Math.abs(parentRect.width - startRect.width) > 2 || Math.abs(parentRect.height - startRect.height) > 2) {
          t = parent;
          break;
        }
        parent = parent.parentElement;
      }
    }
    var cur = t;
    while (cur && cur.nodeType === 1 && !skip(cur)) {
      var j = matchIn(hidden, cur);
      if (j >= 0) return { el: cur, rm: j };
      cur = cur.parentElement;
    }
    return { el: t };
  }

  function send() {
    window.ipc.postMessage(JSON.stringify({ t: 'lists', hidden: hidden, fade: fade }));
  }

  function onMove(e) {
    lastEvent = e;
    var r = resolve(e);
    if (!r) { box.style.display = 'none'; return; }
    var b = r.el.getBoundingClientRect();
    box.style.display = 'block';
    box.style.left = b.left + 'px';
    box.style.top = b.top + 'px';
    box.style.width = b.width + 'px';
    box.style.height = b.height + 'px';
    if (r.rm !== undefined) {
      box.style.borderColor = '#B83232';
      box.style.background = 'rgba(184,50,50,.15)';
    } else {
      box.style.borderColor = '#22D3EE';
      box.style.background = 'rgba(34,211,238,.12)';
    }
  }

  window.addEventListener('keydown', function (e) {
    if (e.key === 'Shift') {
      shiftDown = true;
      if (lastEvent) onMove(lastEvent);
    }
  });
  window.addEventListener('keyup', function (e) {
    if (e.key === 'Shift') {
      shiftDown = false;
      if (lastEvent) onMove(lastEvent);
    }
  });

  function onContext(e) {
    var r = resolve(e);
    if (!r) return;
    var cur = r.el;
    var found = -1;
    while (cur && cur.nodeType === 1 && !skip(cur) && found < 0) {
      found = matchIn(fade, cur);
      cur = cur.parentElement;
    }
    if (found >= 0) {
      fade.splice(found, 1);
    } else {
      var row = U.rowOf(r.el);
      if (row.sel && !fade.some(function (x) { return x.sel === row.sel; })) {
        fade.push({ sel: row.sel, name: (U.nameOf(row.el, false) + ' x' + row.count).slice(0, 60) });
      }
    }
    render();
    send();
  }

  function onClick(e) {
    if (e.type === 'contextmenu') { e.preventDefault(); e.stopPropagation(); e.stopImmediatePropagation(); onContext(e); return; }
    if (e.type !== 'click') {
      e.preventDefault();
      e.stopPropagation();
      e.stopImmediatePropagation();
      return;
    }
    var r = resolve(e);
    if (!r) return;
    e.preventDefault();
    e.stopPropagation();
    e.stopImmediatePropagation();
    if (r.rm !== undefined) {
      hidden.splice(r.rm, 1);
    } else {
      var s = U.pathOf(r.el);
      if (s && !hidden.some(function (x) { return x.sel === s; })) {
        hidden.push({ sel: s, name: U.nameOf(r.el, true) });
      }
    }
    render();
    send();
  }

  ['click', 'mousedown', 'mouseup', 'pointerdown', 'pointerup', 'dblclick', 'auxclick', 'contextmenu', 'submit'].forEach(function (n) {
    window.addEventListener(n, onClick, true);
  });
  window.addEventListener('mousemove', onMove, true);
  window.addEventListener('scroll', function () { box.style.display = 'none'; }, true);

  window.__acApply = function (h, b, op, a, s, edit, fd) {
    hidden = h;
    bgs = b;
    fade = fd || [];
    opacity = op;
    render();
    mount();
    U.setAuto(a);
    window.__ac7Config(s);
  };

  render();
  if (!mount()) {
    var mo = new MutationObserver(function () { if (mount()) mo.disconnect(); });
    mo.observe(document, { childList: true });
  }
  U.setAuto(__AUTO__);
  window.__ac7Config(__SEVEN__);
})();
