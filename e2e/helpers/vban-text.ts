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
 * Mute a routing point (disconnect audio path).
 * Uses Mute property which reliably stops audio flow.
 *
 * NOTE: .Remove is unreliable after re-creation. Use .Mute instead.
 */
export async function muteRoutingPoint(
  host: string,
  slotIn: string,
  channelIn: number,
  slotOut: string,
  channelOut: number,
  port?: number,
): Promise<void> {
  await sendCommand(
    host,
    `Point(${slotIn}.IN[${channelIn}],${slotOut}.OUT[${channelOut}]).Mute = 1;`,
    { port },
  );
}

/**
 * Unmute a routing point (restore audio path).
 */
export async function unmuteRoutingPoint(
  host: string,
  slotIn: string,
  channelIn: number,
  slotOut: string,
  channelOut: number,
  port?: number,
): Promise<void> {
  await sendCommand(
    host,
    `Point(${slotIn}.IN[${channelIn}],${slotOut}.OUT[${channelOut}]).Mute = 0;`,
    { port },
  );
}

/**
 * Disconnect the VASIO8 loopback (mute both channels).
 * This is the standard loopback used by audiotester.
 */
export async function disconnectVasio8Loopback(host: string): Promise<void> {
  await muteRoutingPoint(host, "VASIO8", 1, "VASIO8", 1);
  await new Promise((r) => setTimeout(r, 200));
  await muteRoutingPoint(host, "VASIO8", 2, "VASIO8", 2);
}

/**
 * Reconnect the VASIO8 loopback (unmute both channels).
 * This restores the standard loopback used by audiotester.
 *
 * NOTE: After routing changes, the audiotester monitoring may need
 * a restart (stop + start) to re-establish the ASIO audio stream.
 */
export async function reconnectVasio8Loopback(host: string): Promise<void> {
  await unmuteRoutingPoint(host, "VASIO8", 1, "VASIO8", 1);
  await new Promise((r) => setTimeout(r, 200));
  await unmuteRoutingPoint(host, "VASIO8", 2, "VASIO8", 2);
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
