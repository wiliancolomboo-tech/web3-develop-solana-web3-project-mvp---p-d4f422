import http from 'http';

const healthPort = process.env.PORT || 8080;
http.createServer((req, res) => {
  if (req.url === '/health') {
    res.writeHead(200, { 'Content-Type': 'application/json' });
    res.end(JSON.stringify({
      status: 'UP',
      uptime: process.uptime(),
      timestamp: new Date().toISOString(),
      cluster: process.env.SOLANA_RPC_URL
    }));
  } else {
    res.writeHead(404);
    res.end();
  }
}).listen(healthPort, () => {
  logger.info(`Telemetry healthcheck endpoint online on port ${healthPort}`);
});