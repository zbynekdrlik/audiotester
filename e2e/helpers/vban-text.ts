/**
 * VBAN-TEXT protocol helper for controlling VB-Audio Matrix remotely.
 *
 * VBMatrix listens on UDP 6980 for VBAN-TEXT commands.
 * Commands control routing points (gain, mute, remove).
 *
 * IMPORTANT: No spaces after commas in Point() syntax!
 *   CORRECT: Point(VASIO8.IN[1],VASIO8.OUT[1]).dBGain = 0.0;
 *   WRONG:   Point(VASIO8.IN[1], VASIO8.OUT[1]).dBGain = 0.0;
 *
 * The VASIO8 slot is the virtual ASIO driver used by audiotester.
 * Loopback routing: Point(VASIO8.IN[n],VASIO8.OUT[n]) routes output back to input.
 */

import * as dgram from "dgram";

const VBAN_MAGIC = Buffer.from("VBAN");
const VBAN_TEXT_SR_INDEX = 0x40; // TEXT sub-protocol
const VBAN_TEXT_FORMAT = 0x10; // UTF-8 text
const STREAM_NAME = "Command1";
const DEFAULT_PORT = 6980;

let frameCounter = 0;

/**
 * Build a VBAN-TEXT packet with the given command.
 */
function buildPacket(command: string): Buffer {
  frameCounter++;
  const header = Buffer.alloc(28);

  // Magic bytes
  VBAN_MAGIC.copy(header, 0);
  // SR index | sub-protocol
  header[4] = VBAN_TEXT_SR_INDEX;
  // nSamples, nChannels
  header[5] = 0;
  header[6] = 0;
  // Data format (UTF-8)
  header[7] = VBAN_TEXT_FORMAT;
  // Stream name (16 bytes, null-padded)
  Buffer.from(STREAM_NAME, "ascii").copy(header, 8);
  // Frame counter (4 bytes, little-endian)
  header.writeUInt32LE(frameCounter, 24);

  const payload = Buffer.from(command, "utf-8");
  return Buffer.concat([header, payload]);
}

/**
 * Send a VBAN-TEXT command and optionally wait for a response.
 */
export async function sendCommand(
  host: string,
  command: string,
  options?: { port?: number; waitForResponse?: boolean; timeout?: number },
): Promise<string | null> {
  const port = options?.port ?? DEFAULT_PORT;
  const waitForResponse = options?.waitForResponse ?? false;
  const timeout = options?.timeout ?? 2000;

  return new Promise((resolve, reject) => {
    const socket = dgram.createSocket("udp4");
    const packet = buildPacket(command);
    let responded = false;

    if (waitForResponse) {
      const timer = setTimeout(() => {
        if (!responded) {
          responded = true;
          socket.close();
          resolve(null);
        }
      }, timeout);

      socket.on("message", (msg) => {
        if (!responded) {
          responded = true;
          clearTimeout(timer);
          // Response payload starts after 28-byte header
          const response = msg.subarray(28).toString("utf-8");
          socket.close();
          resolve(response);
        }
      });
    }

    socket.send(packet, port, host, (err) => {
      if (err) {
        socket.close();
        reject(err);
        return;
      }
      if (!waitForResponse) {
        socket.close();
        resolve(null);
      }
    });
  });
}

/**
 * Query a VBMatrix property value.
 */
export async function queryProperty(
  host: string,
  property: string,
  port?: number,
): Promise<string> {
  const response = await sendCommand(host, `${property} = ?;`, {
    port,
    waitForResponse: true,
    timeout: 2000,
  });
  if (!response) {
    throw new Error(`No response from VBMatrix for query: ${property}`);
  }
  // Parse "Property = Value;" format
  const match = response.match(/=\s*(.+?)\s*;?\s*$/);
  return match ? match[1].trim() : response;
}

/**
 * Remove a routing point (disconnect audio path).
 *
 * @param host - VBMatrix host
 * @param slotIn - Input slot SUID (e.g., "VASIO8")
 * @param channelIn - Input channel (1-based)
 * @param slotOut - Output slot SUID (e.g., "VASIO8")
 * @param channelOut - Output channel (1-based)
 */
export async function removeRoutingPoint(
  host: string,
  slotIn: string,
  channelIn: number,
  slotOut: string,
  channelOut: number,
  port?: number,
): Promise<void> {
  await sendCommand(
    host,
    `Point(${slotIn}.IN[${channelIn}],${slotOut}.OUT[${channelOut}]).Remove;`,
    { port },
  );
}

/**
 * Restore a routing point with specified gain.
 *
 * @param host - VBMatrix host
 * @param slotIn - Input slot SUID (e.g., "VASIO8")
 * @param channelIn - Input channel (1-based)
 * @param slotOut - Output slot SUID (e.g., "VASIO8")
 * @param channelOut - Output channel (1-based)
 * @param dBGain - Gain in dB (default: 0.0)
 */
export async function restoreRoutingPoint(
  host: string,
  slotIn: string,
  channelIn: number,
  slotOut: string,
  channelOut: number,
  dBGain: number = 0.0,
  port?: number,
): Promise<void> {
  await sendCommand(
    host,
    `Point(${slotIn}.IN[${channelIn}],${slotOut}.OUT[${channelOut}]).dBGain = ${dBGain.toFixed(1)};`,
    { port },
  );
}

/**
 * Disconnect the VASIO8 loopback (both channels).
 * This is the standard loopback used by audiotester.
 */
export async function disconnectVasio8Loopback(host: string): Promise<void> {
  await removeRoutingPoint(host, "VASIO8", 1, "VASIO8", 1);
  await new Promise((r) => setTimeout(r, 200));
  await removeRoutingPoint(host, "VASIO8", 2, "VASIO8", 2);
}

/**
 * Reconnect the VASIO8 loopback (both channels at 0dB).
 * This restores the standard loopback used by audiotester.
 */
export async function reconnectVasio8Loopback(host: string): Promise<void> {
  await restoreRoutingPoint(host, "VASIO8", 1, "VASIO8", 1, 0.0);
  await new Promise((r) => setTimeout(r, 200));
  await restoreRoutingPoint(host, "VASIO8", 2, "VASIO8", 2, 0.0);
}

/**
 * Check if VBAN-TEXT communication with VBMatrix is working.
 */
export async function isVbanTextAvailable(host: string): Promise<boolean> {
  try {
    const response = await queryProperty(host, "VASIO8.name");
    return response !== "" && !response.includes("Err");
  } catch {
    return false;
  }
}
