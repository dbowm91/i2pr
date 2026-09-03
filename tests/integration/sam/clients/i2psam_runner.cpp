// SPDX-License-Identifier: MIT
//
// Plan 150 runner for the pinned independent i2psam implementation.
// The runner uses StreamSession's normal public API for HELLO, SESSION
// CREATE, STREAM CONNECT, and STREAM ACCEPT.  It detaches only the returned
// raw socket so binary application bytes, including NUL, are not converted
// through i2psam's string-based convenience methods.
//
// Usage:
//   i2psam_runner connect <host> <port> <peer_pub> <send> <expect> <silent>
//                        [<private_file>]
//   i2psam_runner accept  <host> <port> <send> <expect> <silent>
//                        [<private_file>]
//   i2psam_runner import  <host> <port> <private_file> <public_file>

#include <algorithm>
#include <cstdio>
#include <cstdlib>
#include <cstring>
#include <fstream>
#include <string>
#include <sys/socket.h>
#include <thread>
#include <unistd.h>
#include <vector>

#include "i2psam.h"
#include "i2p_base64.h"

namespace {

const std::vector<unsigned char> kTransferBarrier = {
    0x00, 'P', 'L', 'A', 'N', '1', '5', '0', '-', 'T', 'R', 'A', 'N', 'S',
    'F', 'E', 'R', '-', 'D', 'O', 'N', 'E', 0x00};
const std::vector<unsigned char> kTransferAck = {
    0x00, 'P', 'L', 'A', 'N', '1', '5', '0', '-', 'T', 'R', 'A', 'N', 'S',
    'F', 'E', 'R', '-', 'A', 'C', 'K', 0x00};

std::vector<unsigned char> read_file(const std::string &path) {
  std::ifstream fp(path, std::ios::binary);
  if (!fp) {
    std::fprintf(stderr, "i2psam_runner: input file could not be opened\n");
    std::exit(2);
  }
  fp.seekg(0, std::ios::end);
  const std::streamsize size = fp.tellg();
  fp.seekg(0, std::ios::beg);
  if (size < 0) {
    std::fprintf(stderr, "i2psam_runner: input file size unavailable\n");
    std::exit(2);
  }
  std::vector<unsigned char> buf(static_cast<size_t>(size));
  if (size > 0 && !fp.read(reinterpret_cast<char *>(buf.data()), size)) {
    std::fprintf(stderr, "i2psam_runner: input file read failed\n");
    std::exit(2);
  }
  return buf;
}

std::string read_text(const std::string &path) {
  const auto bytes = read_file(path);
  return std::string(bytes.begin(), bytes.end());
}

std::string public_part(const std::string &private_destination) {
  return plan150::public_from_private(private_destination);
}

bool parse_silent(const std::string &value) {
  return value == "true" || value == "TRUE" || value == "1";
}

bool send_bytes(SOCKET fd, const std::vector<unsigned char> &data) {
  size_t sent = 0;
  while (sent < data.size()) {
    const auto *ptr = reinterpret_cast<const char *>(data.data() + sent);
    const size_t request = std::min(data.size() - sent, size_t{4096});
    const ssize_t n = ::send(fd, ptr, request, 0);
    if (n <= 0) return false;
    sent += static_cast<size_t>(n);
    std::this_thread::yield();
  }
  return true;
}

bool receive_bytes(SOCKET fd, size_t expected, std::vector<unsigned char> &out) {
  out.clear();
  out.reserve(expected);
  while (out.size() < expected) {
    unsigned char buffer[16 * 1024];
    const size_t request = std::min(expected - out.size(), sizeof(buffer));
    const ssize_t n = ::recv(fd, reinterpret_cast<char *>(buffer), request, 0);
    if (n <= 0) return false;
    out.insert(out.end(), buffer, buffer + n);
  }
  return true;
}

int transfer(SOCKET fd, const std::vector<unsigned char> &send_data,
             const std::vector<unsigned char> &expected_data) {
  bool send_ok = false;
  std::thread writer([&] {
    send_ok = send_bytes(fd, send_data) && send_bytes(fd, kTransferBarrier);
  });
  std::vector<unsigned char> received;
  const bool receive_ok = receive_bytes(fd, expected_data.size(), received);
  writer.join();
  if (!send_ok) {
    std::fprintf(stderr, "i2psam_runner: binary send failed\n");
    return 5;
  }
  if (!receive_ok) {
    std::fprintf(stderr, "i2psam_runner: binary receive failed\n");
    return 7;
  }
  if (received != expected_data) {
    std::fprintf(stderr, "i2psam_runner: binary payload mismatch\n");
    return 8;
  }
  std::vector<unsigned char> barrier;
  if (!receive_bytes(fd, kTransferBarrier.size(), barrier) ||
      barrier != kTransferBarrier) {
    std::fprintf(stderr, "i2psam_runner: transfer barrier mismatch\n");
    return 8;
  }
  if (!send_bytes(fd, kTransferAck)) {
    std::fprintf(stderr, "i2psam_runner: transfer acknowledgement send failed\n");
    return 5;
  }
  std::vector<unsigned char> acknowledgement;
  if (!receive_bytes(fd, kTransferAck.size(), acknowledgement) ||
      acknowledgement != kTransferAck) {
    std::fprintf(stderr, "i2psam_runner: transfer acknowledgement mismatch\n");
    return 8;
  }
  return 0;
}

int run_connect(const std::string &host, int port, const std::string &peer,
                const std::string &send_path, const std::string &expect_path,
                bool silent, const std::string *private_destination) {
  const auto send_data = read_file(send_path);
  const auto expected_data = read_file(expect_path);
  const std::string destination =
      private_destination == nullptr ? "TRANSIENT" : *private_destination;
  SAM::StreamSession session("i2psam-plan150-connect", host,
                             static_cast<uint16_t>(port), destination);
  auto result = session.connect(peer, silent);
  if (!result.isOk || !result.value) {
    std::fprintf(stderr, "i2psam_runner: STREAM CONNECT rejected\n");
    return 4;
  }
  const SOCKET fd = result.value->release();
  result.value.reset();
  const int rc = transfer(fd, send_data, expected_data);
  ::close(fd);
  return rc;
}

int run_accept(const std::string &host, int port, const std::string &send_path,
               const std::string &expect_path, bool silent,
               const std::string *private_destination) {
  const auto send_data = read_file(send_path);
  const auto expected_data = read_file(expect_path);
  const std::string destination =
      private_destination == nullptr ? "TRANSIENT" : *private_destination;
  SAM::StreamSession session("i2psam-plan150-accept", host,
                             static_cast<uint16_t>(port), destination);
  const std::string private_reply = session.getMyDestination().pub;
  const std::string public_reply = public_part(private_reply);
  if (public_reply.size() != 524) return 3;
  std::fprintf(stdout, "%s\n", public_reply.c_str());
  std::fflush(stdout);

  auto result = session.accept(silent);
  if (!result.isOk || !result.value) {
    std::fprintf(stderr, "i2psam_runner: STREAM ACCEPT rejected\n");
    return 4;
  }
  const SOCKET fd = result.value->release();
  result.value.reset();

  // i2psam consumes only the STREAM STATUS line for a non-silent ACCEPT.
  // i2pr's second line is the authenticated peer Destination, so consume it
  // before handing the descriptor to the binary transfer loop.
  if (!silent) {
    std::string line;
    char byte = 0;
    while (true) {
      const ssize_t n = ::recv(fd, &byte, 1, 0);
      if (n <= 0) {
        ::close(fd);
        return 10;
      }
      if (byte == '\n') break;
      line.push_back(byte);
      if (line.size() > 600) {
        ::close(fd);
        return 10;
      }
    }
    if (line.rfind("DESTINATION=", 0) != 0) {
      ::close(fd);
      return 10;
    }
  }
  const int rc = transfer(fd, send_data, expected_data);
  ::close(fd);
  return rc;
}

int run_import(const std::string &host, int port, const std::string &private_path,
               const std::string &public_path) {
  const std::string private_destination = read_text(private_path);
  const std::string expected_public = read_text(public_path);
  SAM::StreamSession session("i2psam-plan150-import", host,
                             static_cast<uint16_t>(port), private_destination);
  if (session.isSick() || session.getMyDestination().pub.empty()) {
    std::fprintf(stderr, "i2psam_runner: imported SESSION CREATE rejected\n");
    return 3;
  }
  const std::string actual_public = public_part(session.getMyDestination().pub);
  return actual_public == expected_public ? 0 : 8;
}

}  // namespace

int main(int argc, char **argv) {
  if (argc < 2) return 64;
  const std::string role = argv[1];
  try {
    if (role == "connect" && (argc == 8 || argc == 9)) {
      const std::string private_destination =
          argc == 9 ? read_text(argv[8]) : std::string();
      return run_connect(argv[2], std::atoi(argv[3]), argv[4], argv[5], argv[6],
                          parse_silent(argv[7]), argc == 9 ? &private_destination
                                                           : nullptr);
    }
    if (role == "accept" && (argc == 7 || argc == 8)) {
      const std::string private_destination =
          argc == 8 ? read_text(argv[7]) : std::string();
      return run_accept(argv[2], std::atoi(argv[3]), argv[4], argv[5],
                         parse_silent(argv[6]), argc == 8 ? &private_destination
                                                          : nullptr);
    }
    if (role == "import" && argc == 6) {
      return run_import(argv[2], std::atoi(argv[3]), argv[4], argv[5]);
    }
  } catch (...) {
    return 3;
  }
  return 64;
}
