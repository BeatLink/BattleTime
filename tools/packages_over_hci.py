"""Sends packages commands to a watch through a raw H4 HCI serial port, such as the emulator's Bluetooth bridge.

It needs no BlueZ and no root: it speaks HCI to the controller itself, connects to the watch, finds
the packages characteristic, turns its notifications on, and writes each command line, 20 bytes a
write, as the companion app does. See docs/companion-protocol.md.

    python3 -I tools/packages_over_hci.py /path/to/hci '{"c":"apps"}' '{"c":"install","id":"alarm"}'
"""

import os
import select
import sys
import termios
import time
import tty

# ============================================================================
# HCI and ATT
# ============================================================================

COMMAND_PACKET = 0x01
ACL_PACKET = 0x02
EVENT_PACKET = 0x04
COMMAND_COMPLETE = 0x0E
DISCONNECTION_COMPLETE = 0x05
LE_META_EVENT = 0x3E
LE_CONNECTION_COMPLETE = 0x01
RESET = 0x0C03
SET_EVENT_MASK = 0x0C01
LE_CREATE_CONNECTION = 0x200D
ATTRIBUTE_CHANNEL = 0x0004

ERROR_RESPONSE = 0x01
EXCHANGE_MTU_REQUEST = 0x02
EXCHANGE_MTU_RESPONSE = 0x03
READ_BY_TYPE_REQUEST = 0x08
READ_BY_TYPE_RESPONSE = 0x09
WRITE_REQUEST = 0x12
WRITE_RESPONSE = 0x13
NOTIFICATION = 0x1B
CHARACTERISTIC_DECLARATION = 0x2803

# The emulated watch's address, from its FICR.
WATCH_ADDRESS = "C0:DE:0A:1B:2C:3D"
PACKAGES_UUID = bytes.fromhex("6e5400030000400080004e656f54696d")[::-1]
READ_BD_ADDRESS = 0x1009
# What the watch answers a line it could not parse.
GARBLED_REPLY = '{"c":"?"'
# A pause before each write; without it the emulator's bridge controller now and then holds a packet back.
PAUSE_BEFORE_WRITE_SECONDS = 0.3
# How long to wait for an answer before nudging the controller with a harmless command.
NUDGE_AFTER_SECONDS = 2
# Set NEOTIME_HCI_VERBOSE to see each request on standard error.
VERBOSE = bool(os.environ.get("NEOTIME_HCI_VERBOSE"))
# One write's payload at the smallest MTU, as the companion app sends.
WRITE_BYTES = 20


class Disconnected(Exception):
    pass


def command(opcode, parameters=b""):
    return bytes([COMMAND_PACKET, opcode & 0xFF, opcode >> 8, len(parameters)]) + parameters


class HciPort:
    """An H4 serial port, read packet by packet."""

    def __init__(self, path):
        self.port = os.open(path, os.O_RDWR | os.O_NOCTTY)
        tty.setraw(self.port)
        termios.tcflush(self.port, termios.TCIOFLUSH)
        self.buffer = b""

    def write(self, data):
        os.write(self.port, data)

    def packets(self, seconds):
        """Yields (packet type, first header field, payload) until the time runs out."""
        end = time.monotonic() + seconds
        while time.monotonic() < end:
            ready, _, _ = select.select([self.port], [], [], 0.1)
            if ready:
                self.buffer += os.read(self.port, 4096)
            while self.buffer:
                kind = self.buffer[0]
                if kind == EVENT_PACKET and len(self.buffer) >= 3:
                    length = self.buffer[2]
                    if len(self.buffer) < 3 + length:
                        break
                    packet = (EVENT_PACKET, self.buffer[1], self.buffer[3:3 + length])
                    self.buffer = self.buffer[3 + length:]
                    yield packet
                elif kind == ACL_PACKET and len(self.buffer) >= 5:
                    length = int.from_bytes(self.buffer[3:5], "little")
                    if len(self.buffer) < 5 + length:
                        break
                    handle = int.from_bytes(self.buffer[1:3], "little") & 0x0FFF
                    packet = (ACL_PACKET, handle, self.buffer[5:5 + length])
                    self.buffer = self.buffer[5 + length:]
                    yield packet
                elif kind in (EVENT_PACKET, ACL_PACKET):
                    break
                else:
                    self.buffer = self.buffer[1:]

    def wait_for_completion(self, opcode, seconds):
        for kind, code, payload in self.packets(seconds):
            if kind == EVENT_PACKET and code == COMMAND_COMPLETE and int.from_bytes(payload[1:3], "little") == opcode:
                return payload[3:]
        raise TimeoutError(f"no Command Complete for 0x{opcode:04X}")

    def start_controller(self, seconds=60):
        """Sends HCI Reset until it is answered, since the bridge's controller ignores its UART while it boots."""
        end = time.monotonic() + seconds
        while time.monotonic() < end:
            self.write(command(RESET))
            try:
                self.wait_for_completion(RESET, 2)
                break
            except TimeoutError:
                continue
        else:
            raise TimeoutError("the controller never answered HCI Reset")
        # The default mask leaves out LE Meta events, which carry the connection.
        self.write(command(SET_EVENT_MASK, (0x20001FFFFFFFFFFF).to_bytes(8, "little")))
        self.wait_for_completion(SET_EVENT_MASK, 10)


# ============================================================================
# A connection to the watch
# ============================================================================

class Watch:
    def __init__(self, port, handle):
        self.port = port
        self.handle = handle
        self.notified = b""

    @staticmethod
    def connect(port, address=WATCH_ADDRESS, seconds=60):
        peer = bytes(int(part, 16) for part in reversed(address.split(":")))
        # Scan 60 ms in 60 ms, random peer address, 30 ms interval, no latency, 5 s supervision timeout.
        parameters = (0x60).to_bytes(2, "little") * 2 + bytes([0, 1]) + peer + bytes([0])
        parameters += (0x18).to_bytes(2, "little") * 2 + (0).to_bytes(2, "little") + (500).to_bytes(2, "little")
        parameters += (0).to_bytes(2, "little") * 2
        port.write(command(LE_CREATE_CONNECTION, parameters))
        for kind, code, payload in port.packets(seconds):
            if kind == EVENT_PACKET and code == LE_META_EVENT and payload[0] == LE_CONNECTION_COMPLETE:
                if payload[1] != 0:
                    raise RuntimeError(f"connection failed, status 0x{payload[1]:02X}")
                return Watch(port, int.from_bytes(payload[2:4], "little"))
        raise TimeoutError("the watch did not connect")

    def send(self, pdu):
        l2cap = len(pdu).to_bytes(2, "little") + ATTRIBUTE_CHANNEL.to_bytes(2, "little") + pdu
        # Host-to-controller ACL on LE uses packet boundary flag 00.
        self.port.write(bytes([ACL_PACKET]) + self.handle.to_bytes(2, "little")
                        + len(l2cap).to_bytes(2, "little") + l2cap)

    def wait(self, opcodes, seconds):
        """Returns the first ATT PDU with one of the opcodes, or None; keeps notifications on the way."""
        for kind, code, payload in self.port.packets(seconds):
            if kind == EVENT_PACKET and code == DISCONNECTION_COMPLETE:
                raise Disconnected(f"disconnected, reason 0x{payload[3]:02X}")
            if kind == ACL_PACKET and int.from_bytes(payload[2:4], "little") == ATTRIBUTE_CHANNEL:
                pdu = payload[4:]
                if pdu[0] == NOTIFICATION:
                    self.notified += pdu[3:]
                elif pdu[0] in opcodes:
                    return pdu
        return None

    def request(self, pdu, answer, seconds=30):
        if VERBOSE:
            print(f"{time.monotonic():.2f} > {pdu.hex()}", file=sys.stderr)
        self.send(pdu)
        reply = None
        end = time.monotonic() + seconds
        while reply is None and time.monotonic() < end:
            reply = self.wait({answer, ERROR_RESPONSE}, NUDGE_AFTER_SECONDS)
            if reply is None:
                # The emulator's bridge controller sometimes holds a data packet until more HCI traffic comes.
                if VERBOSE:
                    print(f"{time.monotonic():.2f} nudge waiting for 0x{answer:02X}", file=sys.stderr)
                self.port.write(command(READ_BD_ADDRESS))
        if reply is None:
            raise TimeoutError(f"no answer to ATT request 0x{pdu[0]:02X}")
        if reply[0] == ERROR_RESPONSE:
            raise RuntimeError(f"ATT error 0x{reply[4]:02X}")
        return reply

    def packages_handle(self):
        self.request(bytes([EXCHANGE_MTU_REQUEST]) + (128).to_bytes(2, "little"), EXCHANGE_MTU_RESPONSE)
        start = 1
        while start < 0xFFFF:
            answer = self.request(bytes([READ_BY_TYPE_REQUEST]) + start.to_bytes(2, "little")
                                  + (0xFFFF).to_bytes(2, "little")
                                  + CHARACTERISTIC_DECLARATION.to_bytes(2, "little"), READ_BY_TYPE_RESPONSE)
            size = answer[1]
            for offset in range(2, len(answer), size):
                entry = answer[offset:offset + size]
                if entry[5:] == PACKAGES_UUID:
                    return int.from_bytes(entry[3:5], "little")
                start = int.from_bytes(entry[0:2], "little") + 1
        raise RuntimeError("the watch has no packages characteristic")

    def open_packages(self):
        value_handle = self.packages_handle()
        # Its client configuration descriptor follows the value.
        self.request(bytes([WRITE_REQUEST]) + (value_handle + 1).to_bytes(2, "little") + b"\x01\x00", WRITE_RESPONSE)
        return value_handle

    def run_command(self, value_handle, line, seconds=60):
        """Writes one command line and returns the reply line the watch notifies."""
        self.notified = b""
        data = (line + "\n").encode()
        for offset in range(0, len(data), WRITE_BYTES):
            time.sleep(PAUSE_BEFORE_WRITE_SECONDS)
            self.request(bytes([WRITE_REQUEST]) + value_handle.to_bytes(2, "little")
                         + data[offset:offset + WRITE_BYTES], WRITE_RESPONSE, seconds)
        end = time.monotonic() + seconds
        while b"\n" not in self.notified and time.monotonic() < end:
            self.wait(set(), NUDGE_AFTER_SECONDS)
            if b"\n" not in self.notified:
                if VERBOSE:
                    print(f"{time.monotonic():.2f} nudge waiting for a reply", file=sys.stderr)
                self.port.write(command(READ_BD_ADDRESS))
        if b"\n" not in self.notified:
            raise TimeoutError(f"no reply to {line}")
        return self.notified.split(b"\n")[0].decode(errors="replace")


def run_commands(port_path, lines):
    """Connects to the watch through the port and runs each line, giving back the replies."""
    port = HciPort(port_path)
    port.start_controller()
    watch = Watch.connect(port)
    value_handle = watch.open_packages()
    replies = []
    for line in lines:
        reply = watch.run_command(value_handle, line)
        if reply.startswith(GARBLED_REPLY):
            # A packet the bridge held back can arrive out of order and spoil the line; send it again.
            reply = watch.run_command(value_handle, line)
        replies.append(reply)
    return replies


def main():
    if len(sys.argv) < 3:
        sys.exit(__doc__)
    for line, reply in zip(sys.argv[2:], run_commands(sys.argv[1], sys.argv[2:])):
        print(f"{line} -> {reply}")


if __name__ == "__main__":
    main()
