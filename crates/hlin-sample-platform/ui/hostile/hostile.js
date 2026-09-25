// The hostile module: every way out of its frame that the specification
// closes, tried on request, with what happened written down.
//
// It says `ready` and answers heartbeats like any module, so the shell keeps
// it mounted, and does nothing else until a browser test calls one of the
// attempts on `window.hostile`. Each attempt resolves to
//
//   { escaped: <bool>, tried: [{ what, escaped, how }] }
//
// where `escaped` is true when anything it tried got through: a property of
// the parent read, a cookie or a storage area opened, a request that left the
// frame, a script that ran, a window that opened. `how` says what stopped it,
// as far as the frame can tell: an exception's name, or the CSP directive the
// browser reported in a `securitypolicyviolation` event.
//
// Some attempts cannot report, because they end the frame's document:
// navigating itself anywhere. Those the test judges from outside.

(function () {
  'use strict';

  var BRIDGE = [1, 0];
  var sent = 0;
  var initialised = false;
  var platform = null;
  var panel = null;
  var answering = true;
  var waiting = {};
  var violations = [];

  // What the browser says its policy stopped, with the directive that did.
  document.addEventListener('securitypolicyviolation', function (event) {
    violations.push({ uri: event.blockedURI, directive: event.effectiveDirective });
  });

  function send(type, data, re) {
    sent += 1;
    var message = { bridge: BRIDGE, id: 'h-' + sent, type: type, data: data };
    if (re) {
      message.re = re;
    }
    window.parent.postMessage(message, '*');
    return message.id;
  }

  // A `fetch` over the bridge, answered by the page's `response`.
  function bridgeFetch(data) {
    return new Promise(function (resolve) {
      var id = send('fetch', data);
      waiting[id] = resolve;
    });
  }

  function decoded(response) {
    var text = response.body instanceof ArrayBuffer ? new TextDecoder().decode(response.body) : '';
    try {
      return JSON.parse(text);
    } catch (error) {
      return text;
    }
  }

  function wait(ms) {
    return new Promise(function (resolve) {
      setTimeout(resolve, ms);
    });
  }

  // Whether the policy reported blocking this address, and by which
  // directive. Reports are dispatched asynchronously, so this waits a little.
  function policyBlocked(url) {
    return wait(250).then(function () {
      var absolute = new URL(url, document.baseURI).href;
      var hit = violations.filter(function (violation) {
        return violation.uri && (absolute.indexOf(violation.uri) === 0 || violation.uri.indexOf(absolute) === 0);
      })[0];
      return hit ? 'CSP ' + hit.directive : null;
    });
  }

  function record(results) {
    var list = document.getElementById('tried');
    results.forEach(function (result) {
      var item = document.createElement('li');
      item.textContent = (result.escaped ? 'ESCAPED: ' : 'blocked: ') + result.what + ' (' + result.how + ')';
      item.setAttribute('data-escaped', String(result.escaped));
      list.appendChild(item);
    });
    return {
      escaped: results.some(function (result) { return result.escaped; }),
      tried: results,
    };
  }

  // Try `read`; escaped if it returns something, blocked by whatever it threw.
  function reading(what, read) {
    try {
      var value = read();
      return { what: what, escaped: value !== undefined && value !== null && value !== '', how: 'read ' + JSON.stringify(String(value)).slice(0, 80) };
    } catch (error) {
      return { what: what, escaped: false, how: error.name };
    }
  }

  // -- The attempts --

  // The parent's document and anything on it (REQ-1.1: an opaque origin).
  function parentDocument() {
    return record([
      reading('window.parent.document', function () { return window.parent.document.title; }),
      reading('window.top.document', function () { return window.top.document.cookie; }),
      reading('window.parent.location.href', function () { return window.parent.location.href; }),
      reading('window.parent.localStorage', function () { return window.parent.localStorage.length; }),
      // Any property but the few every cross-origin window exposes (its
      // frames by index, `length`, `postMessage` and the like, which are the
      // bridge's own means and say nothing about the page).
      reading('a global on the parent', function () { return typeof window.parent.fetch; }),
      reading('the parent\'s name', function () { return window.parent.name; }),
      reading('window.frameElement', function () { return window.frameElement; }),
    ]);
  }

  // The shell's cookies and storage, from the frame's own document.
  function cookiesAndStorage() {
    var tried = [
      reading('document.cookie', function () { return document.cookie; }),
      reading('localStorage', function () { return window.localStorage.length + ' items'; }),
      reading('sessionStorage', function () { return window.sessionStorage.length + ' items'; }),
    ];
    return new Promise(function (resolve) {
      try {
        var opened = window.indexedDB.open('hlin-hostile');
        opened.onsuccess = function () { resolve({ what: 'indexedDB', escaped: true, how: 'opened' }); };
        opened.onerror = function () { resolve({ what: 'indexedDB', escaped: false, how: String(opened.error && opened.error.name) }); };
      } catch (error) {
        resolve({ what: 'indexedDB', escaped: false, how: error.name });
      }
    }).then(function (indexed) {
      tried.push(indexed);
      var caches;
      try {
        caches = window.caches;
      } catch (error) {
        tried.push({ what: 'caches', escaped: false, how: error.name });
        return tried;
      }
      if (!caches) {
        tried.push({ what: 'caches', escaped: false, how: 'not exposed' });
        return tried;
      }
      return caches.keys().then(
        function () { tried.push({ what: 'caches', escaped: true, how: 'listed' }); return tried; },
        function (error) { tried.push({ what: 'caches', escaped: false, how: error.name }); return tried; }
      );
    }).then(record);
  }

  // A request that leaves the frame: `no-cors`, so a request that was made
  // resolves (opaquely) whether or not the answer could be read, and only a
  // request that was never made rejects.
  function request(what, url, options) {
    return fetch(url, options || { mode: 'no-cors', credentials: 'include' }).then(
      function () { return { what: what, escaped: true, how: 'the request was made' }; },
      function (error) {
        return policyBlocked(url).then(function (policy) {
          return { what: what, escaped: false, how: policy || error.name };
        });
      }
    );
  }

  function byEvent(what, url, start) {
    return new Promise(function (resolve) {
      var settled = false;
      function done(escaped, how) {
        if (!settled) {
          settled = true;
          resolve({ what: what, escaped: escaped, how: how });
        }
      }
      try {
        start(function () { done(true, 'it loaded'); }, function () {
          policyBlocked(url).then(function (policy) { done(false, policy || 'error event'); });
        });
      } catch (error) {
        done(false, error.name);
      }
      setTimeout(function () {
        policyBlocked(url).then(function (policy) { done(false, policy || 'nothing happened'); });
      }, 3000);
    });
  }

  // Anything on the network but this frame's own assets (the module CSP's
  // `connect-src`, `img-src` and the rest).
  function network(targets) {
    var attempts = [
      request('fetch the platform directly', targets.platform),
      request('fetch the shell\'s API', targets.shellApi),
      request('fetch another platform\'s module assets', targets.otherAssets),
      request('fetch somewhere else entirely', targets.elsewhere),
      byEvent('XMLHttpRequest to the platform', targets.platform, function (ok, failed) {
        var xhr = new XMLHttpRequest();
        xhr.open('GET', targets.platform);
        xhr.onload = ok;
        xhr.onerror = failed;
        xhr.send();
      }),
      byEvent('WebSocket to the shell', targets.socket, function (ok, failed) {
        var socket = new WebSocket(targets.socket);
        socket.onopen = ok;
        socket.onerror = failed;
      }),
      byEvent('EventSource to the shell', targets.shellApi, function (ok, failed) {
        var source = new EventSource(targets.shellApi);
        source.onopen = ok;
        source.onerror = function () { source.close(); failed(); };
      }),
      byEvent('an image to the platform', targets.pixel, function (ok, failed) {
        var image = new Image();
        image.onload = ok;
        image.onerror = failed;
        image.src = targets.pixel;
      }),
      Promise.resolve().then(function () {
        var queued = navigator.sendBeacon(targets.beacon, 'x');
        return policyBlocked(targets.beacon).then(function (policy) {
          return { what: 'sendBeacon to the platform', escaped: queued && !policy, how: policy || (queued ? 'queued' : 'refused') };
        });
      }),
    ];
    return Promise.all(attempts).then(record);
  }

  // Script from anywhere but this platform's own assets, and script made
  // from strings (`script-src` without `unsafe-inline` or `unsafe-eval`).
  function scripts(targets) {
    function tag(what, url) {
      return byEvent(what, url, function (ok, failed) {
        var script = document.createElement('script');
        script.src = url;
        script.onload = ok;
        script.onerror = failed;
        document.head.appendChild(script);
      });
    }
    var attempts = [
      tag('a script from the platform directly', targets.lureScript),
      tag('a script from another platform\'s assets', targets.otherScript),
      Promise.resolve().then(function () {
        var inline = document.createElement('script');
        inline.textContent = 'window.lured = "inline";';
        document.head.appendChild(inline);
        return wait(100).then(function () {
          return { what: 'an inline script', escaped: window.lured === 'inline', how: window.lured === 'inline' ? 'it ran' : 'CSP script-src' };
        });
      }),
      Promise.resolve().then(function () {
        try {
          // eslint-disable-next-line no-eval
          var value = eval('"evaluated"');
          return { what: 'eval', escaped: value === 'evaluated', how: 'it ran' };
        } catch (error) {
          return { what: 'eval', escaped: false, how: error.name };
        }
      }),
      Promise.resolve().then(function () {
        try {
          return { what: 'new Function', escaped: new Function('return 1')() === 1, how: 'it ran' };
        } catch (error) {
          return { what: 'new Function', escaped: false, how: error.name };
        }
      }),
      Promise.resolve().then(function () {
        var link = document.createElement('a');
        link.href = 'javascript:window.lured="javascript-url"';
        document.body.appendChild(link);
        link.click();
        return wait(100).then(function () {
          var ran = window.lured === 'javascript-url';
          return { what: 'a javascript: URL', escaped: ran, how: ran ? 'it ran' : 'CSP script-src' };
        });
      }),
      import(targets.lureScript).then(
        function () { return { what: 'import() from the platform', escaped: true, how: 'it loaded' }; },
        function (error) { return { what: 'import() from the platform', escaped: false, how: error.name }; }
      ),
      byEvent('a worker from a blob', 'blob:', function (ok, failed) {
        var url = URL.createObjectURL(new Blob(['postMessage(1)'], { type: 'text/javascript' }));
        var worker = new Worker(url);
        worker.onmessage = ok;
        worker.onerror = failed;
      }),
      byEvent('a frame of its own', targets.lurePage, function (ok, failed) {
        var frame = document.createElement('iframe');
        frame.src = targets.lurePage;
        frame.onload = function () {
          // A blocked frame still fires `load`, on an error page this frame
          // cannot read; a loaded one would be same-origin-readable only if
          // it had escaped the sandbox, so the policy report is the answer.
          policyBlocked(targets.lurePage).then(function (policy) {
            if (policy) { failed(); } else { ok(); }
          });
        };
        document.body.appendChild(frame);
      }),
    ];
    return Promise.all(attempts).then(function (tried) {
      tried.push({ what: 'the lure ran here', escaped: window.lured !== undefined, how: String(window.lured) });
      return record(tried);
    });
  }

  // Another window: a popup, the top, a sibling frame, a form, a dialog.
  function windows(targets) {
    var tried = [];
    var popup = null;
    try {
      popup = window.open(targets.lurePage, '_blank');
      tried.push({ what: 'window.open', escaped: !!popup, how: popup ? 'opened' : 'returned null' });
    } catch (error) {
      tried.push({ what: 'window.open', escaped: false, how: error.name });
    }
    try {
      window.top.location.href = targets.lurePage;
      tried.push({ what: 'navigate the top', escaped: false, how: 'no exception; judged from outside' });
    } catch (error) {
      tried.push({ what: 'navigate the top', escaped: false, how: error.name });
    }
    var anchor = document.createElement('a');
    anchor.href = targets.lurePage;
    anchor.target = '_blank';
    document.body.appendChild(anchor);
    anchor.click();
    tried.push({ what: 'a link to a new window', escaped: false, how: 'clicked; judged from outside' });

    var form = document.createElement('form');
    form.action = targets.lurePage;
    form.target = '_top';
    document.body.appendChild(form);
    try {
      form.submit();
      tried.push({ what: 'a form to the top', escaped: false, how: 'submitted; judged from outside' });
    } catch (error) {
      tried.push({ what: 'a form to the top', escaped: false, how: error.name });
    }

    // Every other frame on the page, navigated and then lied to.
    for (var i = 0; i < window.parent.frames.length; i += 1) {
      var sibling = window.parent.frames[i];
      if (sibling === window) {
        continue;
      }
      try {
        sibling.location.href = targets.lurePage;
        tried.push({ what: 'navigate frame ' + i, escaped: false, how: 'no exception; judged from outside' });
      } catch (error) {
        tried.push({ what: 'navigate frame ' + i, escaped: false, how: error.name });
      }
      try {
        sibling.postMessage({
          bridge: BRIDGE,
          id: 'forged-' + i,
          type: 'context',
          data: { time_range: { from_millis: 0, to_millis: 60000 }, params: {}, generation: 999999 },
        }, '*');
        tried.push({ what: 'pose as the shell to frame ' + i, escaped: false, how: 'posted; judged from outside' });
      } catch (error) {
        tried.push({ what: 'pose as the shell to frame ' + i, escaped: false, how: error.name });
      }
    }

    var started = Date.now();
    var answer = window.confirm('May I?');
    tried.push({
      what: 'a modal dialog',
      escaped: answer === true || Date.now() - started > 500,
      how: 'returned ' + String(answer) + ' at once',
    });

    return policyBlocked(targets.lurePage).then(function (policy) {
      if (policy) {
        tried.push({ what: 'the policy on the lure', escaped: false, how: policy });
      }
      return record(tried);
    });
  }

  // Powerful features the frame's empty `allow` withholds.
  function features() {
    // A feature that asks the person (Firefox's paste menu) answers only
    // when they do; one that has not answered in a few seconds has given
    // the module nothing.
    function unanswered() {
      return new Promise(function (resolve, reject) {
        setTimeout(function () { reject({ name: 'no answer' }); }, 3000);
      });
    }
    function attempt(what, start) {
      try {
        return Promise.race([Promise.resolve(start()), unanswered()]).then(
          function () { return { what: what, escaped: true, how: 'allowed' }; },
          function (error) { return { what: what, escaped: false, how: String(error && error.name) }; }
        );
      } catch (error) {
        return Promise.resolve({ what: what, escaped: false, how: error.name });
      }
    }
    return Promise.all([
      attempt('clipboard write', function () { return navigator.clipboard.writeText('hostile'); }),
      attempt('clipboard read', function () { return navigator.clipboard.readText(); }),
      attempt('fullscreen', function () { return document.documentElement.requestFullscreen(); }),
      attempt('geolocation', function () {
        return new Promise(function (resolve, reject) {
          navigator.geolocation.getCurrentPosition(resolve, function (error) {
            reject({ name: 'code ' + error.code });
          }, { timeout: 2000 });
        });
      }),
    ]).then(record);
  }

  // Through the bridge, at another platform: by a field the page does not
  // read, and by paths that would climb out of this platform's prefix.
  function otherPlatform(other) {
    var asked = [
      { what: 'a platform field naming ' + other, data: { method: 'GET', path: '/api/module/whoami', query: '', headers: {}, platform: other } },
      { what: 'a path climbing to ' + other, data: { method: 'GET', path: '/../' + other + '/api/module/whoami', query: '', headers: {} } },
      { what: 'an encoded climb to ' + other, data: { method: 'GET', path: '/%2e%2e/' + other + '/api/module/whoami', query: '', headers: {} } },
      { what: 'the proxy\'s own address for ' + other, data: { method: 'GET', path: '/p/' + other + '/api/module/whoami', query: '', headers: {} } },
      { what: 'an absolute URL', data: { method: 'GET', path: 'http://127.0.0.1:8082/api/module/whoami', query: '', headers: {} } },
      { what: 'a scheme-relative URL', data: { method: 'GET', path: '//127.0.0.1:8082/api/module/whoami', query: '', headers: {} } },
      { what: 'a query carrying a path', data: { method: 'GET', path: '/api/module/whoami', query: 'x=1#/../../' + other, headers: {} } },
    ];
    return Promise.all(asked.map(function (one) {
      return bridgeFetch(one.data).then(function (response) {
        var body = decoded(response);
        var reached = body && typeof body === 'object' ? body.platform : undefined;
        return {
          what: one.what,
          escaped: reached === other,
          how: response.refusal ? 'refused: ' + response.refusal : 'answered by ' + String(reached) + ' (' + response.status + ')',
          refusal: response.refusal || null,
          reached: reached || null,
        };
      });
    })).then(record);
  }

  // More messages in a second than a frame may send, then a `fetch`; and more
  // fetches at once than a frame may have in flight.
  function flood(count) {
    for (var i = 0; i < count; i += 1) {
      send('notice', { level: 'info', text: 'flood ' + i });
    }
    return bridgeFetch({ method: 'GET', path: '/api/module/whoami', query: '', headers: {} }).then(function (response) {
      return record([{
        what: count + ' messages, then a fetch',
        escaped: !response.refusal,
        how: response.refusal ? 'refused: ' + response.refusal : 'answered ' + response.status,
        refusal: response.refusal || null,
      }]);
    });
  }

  function burst(count) {
    var asked = [];
    for (var i = 0; i < count; i += 1) {
      asked.push(bridgeFetch({ method: 'GET', path: '/api/module/whoami', query: '', headers: {} }));
    }
    return Promise.all(asked).then(function (responses) {
      var refused = responses.filter(function (response) { return response.refusal === 'too_many'; }).length;
      var answered = responses.filter(function (response) { return !response.refusal && response.status === 200; }).length;
      var result = record([{
        what: count + ' fetches at once',
        escaped: refused === 0,
        how: answered + ' answered, ' + refused + ' refused too_many',
      }]);
      result.answered = answered;
      result.refused = refused;
      return result;
    });
  }

  // Streams opened and never pulled: held for as long as the frame lives.
  function hold(count) {
    var asked = [];
    for (var i = 0; i < count; i += 1) {
      asked.push(bridgeFetch({
        method: 'GET',
        path: '/api/module/feed',
        query: 'every_ms=1000',
        headers: { accept: 'text/event-stream' },
        stream: true,
      }));
    }
    return Promise.all(asked).then(function (responses) {
      var open = responses.filter(function (response) { return response.streaming; }).length;
      var refused = responses.filter(function (response) { return response.refusal === 'too_many'; }).length;
      var result = record([{
        what: count + ' streams held open',
        escaped: open >= count,
        how: open + ' open, ' + refused + ' refused too_many',
      }]);
      result.open = open;
      result.refused = refused;
      return result;
    });
  }

  // Announce a change for a panel of another platform, naming that platform.
  function forgeChange(other, otherPanel) {
    send('changed', { panel: otherPanel, platform: other, selections: {} });
    return Promise.resolve(record([{ what: 'a change announced as ' + other, escaped: false, how: 'sent; judged from outside' }]));
  }

  // Each attempt starts in a task of its own, not inside the call that asked
  // for it. A browser test asks through the browser's debugging protocol, and
  // Chromium lets code run from strings while it evaluates what a debugger
  // sent, whatever the page's CSP says: an `eval` made inside that call would
  // succeed where a module's own never could, and prove nothing.
  function own(attempt) {
    return function () {
      var args = arguments;
      return new Promise(function (resolve) {
        setTimeout(function () {
          resolve(attempt.apply(null, args));
        }, 0);
      });
    };
  }

  window.hostile = {
    parentDocument: own(parentDocument),
    cookiesAndStorage: own(cookiesAndStorage),
    network: own(network),
    scripts: own(scripts),
    windows: own(windows),
    features: own(features),
    otherPlatform: own(otherPlatform),
    flood: own(flood),
    burst: own(burst),
    hold: own(hold),
    forgeChange: own(forgeChange),
    // One request straight from the frame, as the frame's own `fetch` makes
    // it: status if it was answered and readable, else the error's name.
    // For the tests that take the module CSP away to see what stands behind
    // it.
    direct: own(function (url, init) {
      return fetch(url, init).then(
        function (response) { return { status: response.status }; },
        function (error) { return { error: error.name }; }
      );
    }),
    // Leave the frame, for the test to judge from outside.
    navigate: own(function (url) {
      window.location.href = url;
    }),
    // Wedge the frame's thread, starting after this call has returned, so the
    // test can watch the page while it spins.
    spin: function (ms) {
      setTimeout(function () {
        var until = Date.now() + ms;
        while (Date.now() < until) {
          // Nothing: that is the point.
        }
      }, 50);
    },
    // Throw, synchronously in a task and as a rejected promise.
    fail: function () {
      setTimeout(function () {
        throw new Error('the hostile module threw');
      }, 0);
      Promise.reject(new Error('the hostile module rejected'));
    },
    stopAnswering: function () {
      answering = false;
    },
    violations: function () {
      return violations.slice();
    },
    who: function () {
      return { platform: platform, panel: panel };
    },
  };

  window.addEventListener('message', function (event) {
    if (event.source !== window.parent) {
      return;
    }
    var message = event.data;
    if (!message || typeof message !== 'object' || !message.data) {
      return;
    }
    switch (message.type) {
      case 'init':
        if (initialised) {
          return;
        }
        initialised = true;
        platform = message.data.platform;
        panel = message.data.panel;
        send('ready', { kit: 'hand-written' });
        document.getElementById('state').setAttribute('data-state', 'ready');
        document.getElementById('state').textContent = 'ready, as ' + platform + '/' + panel;
        break;
      case 'heartbeat':
        if (answering) {
          send('heartbeat', { n: message.data.n }, message.id);
        }
        break;
      case 'response':
        if (waiting[message.re]) {
          var resolve = waiting[message.re];
          delete waiting[message.re];
          resolve(message.data);
        }
        break;
      default:
        break;
    }
  });
})();
