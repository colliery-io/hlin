// A page module: the smallest one that shows what the shell told it.
//
// It does what the specification asks of any module — acts on the first
// `init`, says `ready`, echoes every `heartbeat` — and shows what a page is
// told that a panel is not: `init.page` set, and the navigation entry's path
// where a panel's key would be. It asks its platform who is looking, through
// the shell like any module, and has one button that asks the shell to go
// back to a panel, for the browser tests.

(function () {
  'use strict';

  var BRIDGE = [1, 0];
  var sent = 0;
  var initialised = false;
  var platform = null;
  var whoami = null;

  function send(type, data, re) {
    sent += 1;
    var message = { bridge: BRIDGE, id: 'm-' + sent, type: type, data: data };
    if (re) {
      message.re = re;
    }
    // `*`, because an opaque origin cannot name its parent's (REQ-2.2).
    window.parent.postMessage(message, '*');
    return message.id;
  }

  function show(id, text) {
    document.getElementById(id).textContent = text;
  }

  function theme(data) {
    var tokens = data.tokens || {};
    Object.keys(tokens).forEach(function (name) {
      document.documentElement.style.setProperty(name, tokens[name]);
    });
    document.documentElement.style.colorScheme = data.scheme;
  }

  function context(data) {
    var range = data.time_range;
    show('range', range ? Math.round((range.to_millis - range.from_millis) / 60000) + ' minutes' : 'none');
  }

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
        document.body.setAttribute('data-instance', message.data.instance);
        document.body.setAttribute('data-page', String(message.data.page));
        document.body.setAttribute('data-path', message.data.panel);
        show(
          'opened',
          (message.data.page ? 'a page' : 'a panel') + ', ' + message.data.panel + ' of ' + platform,
        );
        show('viewer', (message.data.viewer && message.data.viewer.name) || 'someone unnamed');
        show('restored', message.data.restored === null ? 'nothing' : 'something');
        context(message.data.context || {});
        theme(message.data.theme || {});
        send('ready', { kit: 'hand-written' });
        document.getElementById('state').setAttribute('data-state', 'ready');
        show('state', 'ready');
        whoami = send('fetch', {
          method: 'GET',
          path: '/api/module/whoami',
          query: '',
          headers: { accept: 'application/json' },
        });
        break;

      case 'heartbeat':
        send('heartbeat', { n: message.data.n }, message.id);
        break;

      case 'response':
        if (message.re !== whoami) {
          return;
        }
        if (message.data.refusal) {
          show('whoami', 'the shell refused: ' + message.data.refusal);
          return;
        }
        try {
          var body = JSON.parse(new TextDecoder().decode(message.data.body));
          show('whoami', body.principal + ' on ' + body.platform + ' (' + message.data.status + ')');
        } catch (error) {
          show('whoami', 'an unreadable answer (' + message.data.status + ')');
        }
        break;

      case 'context':
        context(message.data);
        break;

      case 'theme':
        theme(message.data);
        break;

      default:
        // Anything else, including types from a newer minor, is ignored.
        break;
    }
  });

  document.getElementById('to-panel').addEventListener('click', function () {
    send('navigate', { to: { platform: platform, panel: 'module-probe' } });
  });
})();
