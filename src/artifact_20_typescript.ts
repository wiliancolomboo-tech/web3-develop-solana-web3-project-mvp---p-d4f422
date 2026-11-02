import * as http from 'http';

const healthPort = process.env.PORT || 8080;
http.createServer((req, res) => {
  if (req.url === '/health') {
    res.writeHead(200, { 'Content-Type': 'application/json' });
    res.end(JSON.stringify({ status: 'HEALTHY', timestamp: new Date().toISOString() }));
  } else {
    res.writeHead(404);
    res.end();
  }
}).listen(healthPort, () => {
  console.log(`[HealthCheck] Health endpoint responding on port ${healthPort}`);
});