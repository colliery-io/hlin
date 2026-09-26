// A forward proxy onto the containerised twenty's network, for the tests only.
//
// The widgets' own UIs are published nowhere: in deploy/twenty/compose.yml a
// widget is reachable only on the compose network, as `clock.comp.test:8080`,
// the way a platform's UI is reachable only at its own host. So to open one in
// a browser on this machine, `angreal e2e twenty --against compose` runs this
// in a container on that network, publishes it on loopback while the suite
// runs, and the suite gives the contexts that open own UIs this as their
// proxy. The browser then asks for `http://clock.comp.test:8080/` by that
// name, and the request is made from inside the network.
//
// Plain http, forwarded or tunnelled with CONNECT (which Playwright's own
// request client asks for), and only to `*.comp.test`: anything else is
// refused. Bodies are piped both ways as they arrive, so a streamed read (the
// deploy log's) streams.

const http = require('http');
const net = require('net');

const PORT = Number(process.env.PORT || 3128);
const ALLOWED = /\.comp\.test$/;

const server = http.createServer((request, response) => {
  let target;
  try {
    target = new URL(request.url);
  } catch {
    response.writeHead(400, { 'content-type': 'text/plain' }).end('an absolute URL, please\n');
    return;
  }
  if (target.protocol !== 'http:' || !ALLOWED.test(target.hostname)) {
    response.writeHead(403, { 'content-type': 'text/plain' }).end(`not forwarded: ${target.host}\n`);
    return;
  }

  const headers = { ...request.headers };
  delete headers['proxy-connection'];
  delete headers['proxy-authorization'];
  const upstream = http.request(
    {
      host: target.hostname,
      port: target.port || 80,
      path: target.pathname + target.search,
      method: request.method,
      headers,
    },
    (answer) => {
      response.writeHead(answer.statusCode, answer.rawHeaders);
      answer.pipe(response);
    },
  );
  upstream.on('error', (error) => {
    if (!response.headersSent) {
      response.writeHead(502, { 'content-type': 'text/plain' });
    }
    response.end(`${target.host}: ${error.message}\n`);
  });
  // The browser gone (a page navigated away from a stream): so is the
  // request it made.
  response.on('close', () => upstream.destroy());
  request.pipe(upstream);
});

// A tunnel: what Playwright's own request client asks for, even for plain
// http. Only to `*.comp.test`, like the rest.
server.on('connect', (request, socket, head) => {
  const [host, port] = request.url.split(':');
  if (!ALLOWED.test(host) || !port) {
    socket.end('HTTP/1.1 403 Forbidden\r\n\r\n');
    return;
  }
  const upstream = net.connect(Number(port), host, () => {
    socket.write('HTTP/1.1 200 Connection Established\r\n\r\n');
    upstream.write(head);
    upstream.pipe(socket);
    socket.pipe(upstream);
  });
  upstream.on('error', () => socket.end('HTTP/1.1 502 Bad Gateway\r\n\r\n'));
  socket.on('error', () => upstream.destroy());
});

server.listen(PORT, '0.0.0.0', () => console.log(`forwarding to *.comp.test on ${PORT}`));
