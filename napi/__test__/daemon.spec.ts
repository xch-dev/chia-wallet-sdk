import test from "ava";
import { createHash } from "node:crypto";
import { type AddressInfo, createServer as createTcpServer } from "node:net";
import { createServer, type TLSSocket } from "node:tls";

import { Certificate, Connector, DaemonClient } from "../index.js";

// There's no Chia daemon in CI, so these tests run against a fake one: a TLS server that
// speaks just enough of the websocket protocol for the client.

const WEBSOCKET_GUID = "258EAFA5-E914-47DA-95CA-C5AB0DC85B11";

// Generating a certificate is slow in debug builds, so the server and clients share one.
let certificate: Certificate | undefined;
function sharedCertificate() {
  certificate ??= Certificate.generate();
  return certificate;
}

interface Frame {
  opcode: number;
  payload: Buffer;
}

class FakeConnection {
  private buffer = Buffer.alloc(0);
  private frames: Frame[] = [];
  private waiters: ((frame: Frame | null) => void)[] = [];
  private ended = false;
  private socket: TLSSocket;

  constructor(socket: TLSSocket) {
    this.socket = socket;
    socket.on("data", (data: Buffer) => {
      this.buffer = Buffer.concat([this.buffer, data]);
      this.parseFrames();
    });
    socket.on("close", () => {
      this.ended = true;
      for (const waiter of this.waiters.splice(0)) waiter(null);
    });
  }

  private parseFrames() {
    while (this.buffer.length >= 2) {
      const opcode = this.buffer[0] & 0x0f;
      const masked = (this.buffer[1] & 0x80) !== 0;
      let length = this.buffer[1] & 0x7f;
      let offset = 2;

      if (length === 126) {
        if (this.buffer.length < 4) return;
        length = this.buffer.readUInt16BE(2);
        offset = 4;
      } else if (length === 127) {
        if (this.buffer.length < 10) return;
        length = Number(this.buffer.readBigUInt64BE(2));
        offset = 10;
      }

      const maskOffset = offset;
      if (masked) offset += 4;
      if (this.buffer.length < offset + length) return;

      const payload = Buffer.from(
        this.buffer.subarray(offset, offset + length),
      );
      if (masked) {
        for (let i = 0; i < payload.length; i++) {
          payload[i] ^= this.buffer[maskOffset + (i % 4)];
        }
      }
      this.buffer = this.buffer.subarray(offset + length);

      const frame = { opcode, payload };
      const waiter = this.waiters.shift();
      if (waiter) waiter(frame);
      else this.frames.push(frame);
    }
  }

  private nextFrame(): Promise<Frame | null> {
    const frame = this.frames.shift();
    if (frame) return Promise.resolve(frame);
    if (this.ended) return Promise.resolve(null);
    return new Promise((resolve) => this.waiters.push(resolve));
  }

  private sendFrame(opcode: number, payload: Buffer) {
    let header: Buffer;
    if (payload.length < 126) {
      header = Buffer.from([0x80 | opcode, payload.length]);
    } else {
      header = Buffer.alloc(4);
      header[0] = 0x80 | opcode;
      header[1] = 126;
      header.writeUInt16BE(payload.length, 2);
    }
    this.socket.write(Buffer.concat([header, payload]));
  }

  async receiveJson(): Promise<any> {
    while (true) {
      const frame = await this.nextFrame();
      if (frame === null) throw new Error("connection ended");
      if (frame.opcode === 1) return JSON.parse(frame.payload.toString("utf8"));
      if (frame.opcode === 8) throw new Error("unexpected close frame");
    }
  }

  sendJson(value: unknown) {
    this.sendFrame(1, Buffer.from(JSON.stringify(value), "utf8"));
  }

  /** Drops the connection without a close frame, as if the daemon crashed. */
  drop() {
    this.socket.destroy();
  }

  /** Waits for the client's close frame, replies to it, and ends the connection. */
  async expectClose() {
    while (true) {
      const frame = await this.nextFrame();
      if (frame === null)
        throw new Error("connection ended without a close frame");
      if (frame.opcode === 8) {
        this.sendFrame(8, frame.payload);
        this.socket.end();
        return;
      }
    }
  }
}

async function startFakeDaemon() {
  const cert = sharedCertificate();
  const connections: FakeConnection[] = [];
  const waiters: ((connection: FakeConnection) => void)[] = [];

  const server = createServer(
    { cert: cert.certPem, key: cert.keyPem },
    (socket) => {
      let request = Buffer.alloc(0);

      const onData = (data: Buffer) => {
        request = Buffer.concat([request, data]);
        const end = request.indexOf("\r\n\r\n");
        if (end === -1) return;
        socket.off("data", onData);

        const key = /sec-websocket-key: *(.+)\r\n/i
          .exec(request.toString("latin1"))![1]
          .trim();
        const accept = createHash("sha1")
          .update(key + WEBSOCKET_GUID)
          .digest("base64");
        socket.write(
          "HTTP/1.1 101 Switching Protocols\r\nUpgrade: websocket\r\nConnection: Upgrade\r\n" +
            `Sec-WebSocket-Accept: ${accept}\r\n\r\n`,
        );

        const connection = new FakeConnection(socket);
        const rest = request.subarray(end + 4);
        if (rest.length > 0) socket.emit("data", rest);

        const waiter = waiters.shift();
        if (waiter) waiter(connection);
        else connections.push(connection);
      };

      socket.on("data", onData);
    },
  );

  await new Promise<void>((resolve) => server.listen(0, "127.0.0.1", resolve));
  const { port } = server.address() as AddressInfo;

  return {
    url: `wss://127.0.0.1:${port}`,
    accept(): Promise<FakeConnection> {
      const connection = connections.shift();
      if (connection) return Promise.resolve(connection);
      return new Promise((resolve) => waiters.push(resolve));
    },
    close() {
      server.close();
    },
  };
}

function connector() {
  return new Connector(sharedCertificate());
}

async function expectRegistration(connection: FakeConnection) {
  const request = await connection.receiveJson();
  if (
    request.command !== "register_service" ||
    request.destination !== "daemon"
  ) {
    throw new Error(`unexpected request: ${JSON.stringify(request)}`);
  }
  return request.data.service as string;
}

test("daemon client forwards requests and events through a fake daemon", async (t) => {
  const daemon = await startFakeDaemon();
  t.teardown(() => daemon.close());

  const [client, connection] = await Promise.all([
    DaemonClient.connect(daemon.url, connector(), 10),
    daemon.accept(),
  ]);

  const origin = await expectRegistration(connection);
  t.true(origin.startsWith("chia-wallet-sdk-"));

  const subscription = await client.subscribe("wallet_ui");
  t.is(await expectRegistration(connection), "wallet_ui");

  // Full node requests go through the daemon to the `chia_full_node` service.
  const answer = (async () => {
    const request = await connection.receiveJson();
    t.is(request.command, "get_network_info");
    t.is(request.destination, "chia_full_node");
    t.is(request.origin, origin);
    connection.sendJson({
      command: "get_network_info",
      ack: true,
      origin: "chia_full_node",
      destination: origin,
      request_id: request.request_id,
      data: {
        success: true,
        network_name: "testnet11",
        network_prefix: "txch",
      },
    });
  })();

  const rpc = client.rpcClient();
  t.is(rpc.baseUrl(), daemon.url);
  const [networkInfo] = await Promise.all([rpc.getNetworkInfo(), answer]);
  t.true(networkInfo.success);
  t.is(networkInfo.networkName, "testnet11");
  t.is(networkInfo.networkPrefix, "txch");

  connection.sendJson({
    command: "state_changed",
    ack: false,
    origin: "chia_wallet",
    destination: "wallet_ui",
    request_id: "",
    data: { state: "sync_changed", wallet_id: 1 },
  });

  const event = await subscription.next();
  t.truthy(event);
  t.is(event!.command, "state_changed");
  t.is(event!.origin, "chia_wallet");
  t.deepEqual(JSON.parse(event!.data), { state: "sync_changed", wallet_id: 1 });

  // After a clean close, the subscription ends and the client doesn't reconnect.
  await Promise.all([client.close(), connection.expectClose()]);
  t.is(await subscription.next(), null);
  t.false(await client.nextDisconnect());
  t.false(await client.nextReconnect());
  await t.throwsAsync(rpc.getNetworkInfo(), { message: /Connection closed/ });
});

async function connectClient(
  daemon: Awaited<ReturnType<typeof startFakeDaemon>>,
) {
  const [client, connection] = await Promise.all([
    DaemonClient.connect(daemon.url, connector(), 10),
    daemon.accept(),
  ]);
  const origin = await expectRegistration(connection);
  return { client, connection, origin };
}

/** Answers the next full node request with the network info of testnet11. */
async function answerNetworkInfo(connection: FakeConnection, origin: string) {
  const request = await connection.receiveJson();
  if (request.command !== "get_network_info") {
    throw new Error(`unexpected request: ${JSON.stringify(request)}`);
  }
  connection.sendJson({
    command: "get_network_info",
    ack: true,
    origin: "chia_full_node",
    destination: origin,
    request_id: request.request_id,
    data: { success: true, network_name: "testnet11", network_prefix: "txch" },
  });
}

test("daemon subscription reports skipped events and keeps working", async (t) => {
  const daemon = await startFakeDaemon();
  t.teardown(() => daemon.close());

  const { client, connection, origin } = await connectClient(daemon);
  const subscription = await client.subscribe("wallet_ui");
  t.is(await expectRegistration(connection), "wallet_ui");

  // More events arrive than the subscription holds before it reads any of them.
  const count = 300;
  for (let index = 0; index < count; index++) {
    connection.sendJson({
      command: "state_changed",
      ack: false,
      origin: "chia_wallet",
      destination: "wallet_ui",
      request_id: "",
      data: { index },
    });
  }

  // The client reads messages in order, so once this response arrives, so have the events.
  const rpc = client.rpcClient();
  await Promise.all([
    rpc.getNetworkInfo(),
    answerNetworkInfo(connection, origin),
  ]);

  const skipped = count - 256;
  await t.throwsAsync(subscription.next(), {
    message: new RegExp(`skipped ${skipped} events`),
  });

  const event = await subscription.next();
  t.truthy(event);
  t.deepEqual(JSON.parse(event!.data), { index: skipped });

  await Promise.all([client.close(), connection.expectClose()]);
});

test("daemon client reconnects after the connection drops", async (t) => {
  const daemon = await startFakeDaemon();
  t.teardown(() => daemon.close());

  const { client, connection, origin } = await connectClient(daemon);
  await client.subscribe("wallet_ui");
  t.is(await expectRegistration(connection), "wallet_ui");

  // Without a close frame, the client reconnects and registers its services again.
  connection.drop();
  const reconnected = await daemon.accept();
  t.true(await client.nextDisconnect());
  t.is(await expectRegistration(reconnected), origin);
  t.is(await expectRegistration(reconnected), "wallet_ui");
  t.true(await client.nextReconnect());

  const rpc = client.rpcClient();
  const [networkInfo] = await Promise.all([
    rpc.getNetworkInfo(),
    answerNetworkInfo(reconnected, origin),
  ]);
  t.is(networkInfo.networkName, "testnet11");

  await Promise.all([client.close(), reconnected.expectClose()]);
});

test("daemon client fails to connect to a closed port", async (t) => {
  // Bind a port and release it, so that nothing is listening on it.
  const server = createTcpServer();
  await new Promise<void>((resolve) => server.listen(0, "127.0.0.1", resolve));
  const { port } = server.address() as AddressInfo;
  await new Promise<void>((resolve) => server.close(() => resolve()));

  await t.throwsAsync(
    DaemonClient.connect(`wss://127.0.0.1:${port}`, connector(), 5),
  );
});

test("daemon client rejects an invalid timeout", async (t) => {
  await t.throwsAsync(
    DaemonClient.connect("wss://127.0.0.1:1", connector(), -1),
    {
      message: /Invalid timeout/,
    },
  );
});
