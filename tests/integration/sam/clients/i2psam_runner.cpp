// SPDX-License-Identifier: MIT
//
// Plan 150 §6 — i2psam runner.
//
// Drives the i2pr SAM 3.1 listener through i2psam (snapshot
// b80ecd487f7b8d1a743a1f40337b2eb0caaae6ac) without linking any
// i2pr crate. This file is harness code; it intentionally calls
// i2psam's normal public API only.
//
// Usage:
//   i2psam_runner connect <sam_host> <sam_port> <peer_pub>
//                     <send_payload_file> <expect_payload_file> <silent:true|false>
//   i2psam_runner accept  <sam_host> <sam_port>
//                     <send_payload_file> <expect_payload_file> <silent:true|false>

#include <chrono>
#include <cstdio>
#include <cstdlib>
#include <cstring>
#include <errno.h>
#include <fstream>
#include <iostream>
#include <memory>
#include <string>
#include <sys/socket.h>
#include <sys/types.h>
#include <thread>
#include <unistd.h>
#include <vector>

#include "i2psam.h"

namespace {

std::vector<unsigned char> read_file(const std::string &path) {
  std::ifstream fp(path, std::ios::binary);
  if (!fp) {
    std::fprintf(stderr, "i2psam_runner: cannot open %s\n", path.c_str());
    std::exit(2);
  }
  fp.seekg(0, std::ios::end);
  std::streamsize size = fp.tellg();
  fp.seekg(0, std::ios::beg);
  std::vector<unsigned char> buf(static_cast<size_t>(size));
  if (size > 0 && !fp.read(reinterpret_cast<char *>(buf.data()), size)) {
    std::fprintf(stderr, "i2psam_runner: short read on %s\n", path.c_str());
    std::exit(2);
  }
  return buf;
}

int compare_bytes(const std::vector<unsigned char> &got,
                  const std::vector<unsigned char> &want) {
  if (got.size() != want.size()) {
    std::fprintf(stderr, "i2psam_runner: length mismatch got=%zu want=%zu\n",
                 got.size(), want.size());
    return -1;
  }
  if (got != want) {
    std::fprintf(stderr, "i2psam_runner: payload content mismatch\n");
    return -1;
  }
  return 0;
}

int run_connect(const std::string &host, int port, const std::string &peer_pub,
                const std::string &send_path, const std::string &expect_path,
                bool silent) {
  auto send_buf = read_file(send_path);
  auto expect_buf = read_file(expect_path);

  const std::string nickname = "i2psam-runner-connect";
  SAM::StreamSession session(nickname, host, static_cast<uint16_t>(port),
                              /*destination=*/"TRANSIENT");

  // Plan 150 §6: publish the connector's public destination on stdout
  // line 1 for trace symmetry; only the accepter's pub is required to
  // drive a CONNECT, but emitting ours simplifies orchestrator debug.
  // See the accept path for the slicing rationale.
  const std::string &raw_dest = session.getMyDestination().pub;
  const size_t pub_chars = std::min<size_t>(raw_dest.size(), 522u);
  std::string my_pub = raw_dest.substr(0, pub_chars);
  my_pub.append("==");
  std::fwrite(my_pub.data(), 1, my_pub.size(), stdout);
  std::fputc('\n', stdout);
  std::fflush(stdout);

  auto conn_result = session.connect(peer_pub, silent);
  if (!conn_result.isOk) {
    std::fprintf(stderr, "i2psam_runner: STREAM CONNECT failed\n");
    return 4;
  }
  auto conn = std::move(conn_result.value);

  // i2psam's I2pSocket::read uses std::string(buffer) which truncates
  // at the first NUL byte. We bypass it by releasing the underlying
  // socket fd and using POSIX read() / write() / shutdown() directly.
  // After `release()` returns, the I2pSocket keeps socket_ ==
  // INVALID_SOCKET so its destructor is a no-op.
  std::unique_ptr<SAM::I2pSocket> raw_socket(conn.release());
  SOCKET raw_fd = raw_socket->release();
  // Releasing the fd detaches it from the I2pSocket's internal state, so
  // the wrapper can be reclaimed without double-closing the fd.

  // Send the payload via POSIX write().
  const char *send_ptr = reinterpret_cast<const char *>(send_buf.data());
  size_t send_remaining = send_buf.size();
  while (send_remaining > 0) {
    ssize_t n = ::send(raw_fd, send_ptr, send_remaining, 0);
    if (n <= 0) {
      std::fprintf(stderr, "i2psam_runner: send failed\n");
      ::close(raw_fd);
      return 5;
    }
    send_ptr += n;
    send_remaining -= static_cast<size_t>(n);
  }
  // Do NOT shutdown(SHUT_WR) here: i2pr's SAM bridge interprets
  // shutdown as an EOF on its read side and closes the raw stream
  // immediately, preventing the peer (which is sending a delayed
  // echo) from getting its bytes through. The socket close at the
  // end of this function will be the canonical EOF signal.

  std::vector<unsigned char> recv_buf;
  recv_buf.reserve(expect_buf.size());
  // Use POSIX recv() directly to bypass i2psam's std::string(buffer)
  // bug where leading NUL bytes truncate the returned string.
  while (recv_buf.size() < expect_buf.size()) {
    unsigned char tmp[4096];
    ssize_t n = ::recv(raw_fd, tmp, sizeof(tmp), 0);
    if (n == 0) {
      std::fprintf(stderr, "i2psam_runner: read returned empty after %zu bytes\n",
                   recv_buf.size());
      ::close(raw_fd);
      return 7;
    }
    if (n < 0) {
      std::fprintf(stderr, "i2psam_runner: recv failed: %s\n", strerror(errno));
      ::close(raw_fd);
      return 7;
    }
    recv_buf.insert(recv_buf.end(), tmp, tmp + n);
    if (recv_buf.size() >= expect_buf.size()) break;
  }
  ::close(raw_fd);
  int result = compare_bytes(recv_buf, expect_buf) == 0 ? 0 : 8;
  return result;
}

int run_accept(const std::string &host, int port, const std::string &send_path,
               const std::string &expect_path, bool silent) {
  auto send_buf = read_file(send_path);
  auto expect_buf = read_file(expect_path);

  const std::string nickname = "i2psam-runner-accept";
  SAM::StreamSession session(nickname, host, static_cast<uint16_t>(port),
                              /*destination=*/"TRANSIENT");

  // Plan 150 §6: publish the accepter's public destination on stdout
  // line 1 so the orchestrator can drive a cross-client STREAM CONNECT
  // against it. i2psam's getMyDestination().pub returns the full private
  // destination string (608 chars for Ed25519) with one trailing `=`
  // padding char. i2pr's STREAM CONNECT expects the public portion
  // (391 bytes / 524 Base64 chars) with two trailing `=` padding chars.
  // Slice the first 522 chars (the public portion before the trailing
  // padding) and append `==` so the encoded form is byte-exact for
  // STREAM CONNECT. Flush before blocking in STREAM ACCEPT so the
  // orchestrator is not left waiting.
  const std::string &raw_dest = session.getMyDestination().pub;
  const size_t pub_chars = std::min<size_t>(raw_dest.size(), 522u);
  std::string my_pub = raw_dest.substr(0, pub_chars);
  my_pub.append("==");
  std::fwrite(my_pub.data(), 1, my_pub.size(), stdout);
  std::fputc('\n', stdout);
  std::fflush(stdout);

  auto conn_result = session.accept(silent);
  if (!conn_result.isOk) {
    std::fprintf(stderr, "i2psam_runner: STREAM ACCEPT failed\n");
    return 4;
  }
  auto conn = std::move(conn_result.value);

  // Bypass i2psam's std::string(buffer) bug (truncates at first NUL
  // byte) by releasing the underlying socket fd and reading bytes
  // directly with POSIX recv().
  SOCKET raw_fd = conn->release();
  conn.reset();

  // Per the SAM 3.1 wire format, the non-silent STREAM ACCEPT reply is:
  //   STREAM STATUS RESULT=OK\n
  //   DESTINATION=<authenticated peer public destination>\n
  //   <raw bytes>
  // i2psam's accept() consumes only the STATUS line. Read the
  // DESTINATION= line so subsequent reads see only the peer's payload.
  std::string dest_line;
  while (true) {
    char c;
    ssize_t n = ::recv(raw_fd, &c, 1, 0);
    if (n <= 0) {
      std::fprintf(stderr,
                   "i2psam_runner: short read while consuming DESTINATION= line\n");
      ::close(raw_fd);
      return 10;
    }
    if (c == '\n') break;
    dest_line.push_back(c);
  }
  const std::string dest_prefix = "DESTINATION=";
  if (dest_line.size() < dest_prefix.size() ||
      dest_line.compare(0, dest_prefix.size(), dest_prefix) != 0) {
    std::fprintf(stderr,
                 "i2psam_runner: expected DESTINATION= line after ACCEPT, got %zu bytes\n",
                 dest_line.size());
    ::close(raw_fd);
    return 10;
  }

  std::vector<unsigned char> recv_buf;
  recv_buf.reserve(expect_buf.size());
  while (recv_buf.size() < expect_buf.size()) {
    unsigned char tmp[4096];
    ssize_t n = ::recv(raw_fd, tmp, sizeof(tmp), 0);
    if (n == 0) {
      std::fprintf(stderr, "i2psam_runner: accept read returned empty after %zu bytes\n",
                   recv_buf.size());
      ::close(raw_fd);
      return 7;
    }
    if (n < 0) {
      std::fprintf(stderr, "i2psam_runner: recv failed: %s\n", strerror(errno));
      ::close(raw_fd);
      return 7;
    }
    recv_buf.insert(recv_buf.end(), tmp, tmp + n);
    if (recv_buf.size() >= expect_buf.size()) break;
  }
  int result = 0;
  if (compare_bytes(recv_buf, expect_buf) != 0) result = 8;

  const char *send_ptr = reinterpret_cast<const char *>(send_buf.data());
  size_t send_remaining = send_buf.size();
  while (send_remaining > 0) {
    ssize_t n = ::send(raw_fd, send_ptr, send_remaining, 0);
    if (n <= 0) {
      std::fprintf(stderr, "i2psam_runner: send failed\n");
      ::close(raw_fd);
      return 9;
    }
    send_ptr += n;
    send_remaining -= static_cast<size_t>(n);
  }
  // See the connect path for why we do NOT shutdown(SHUT_WR).
  ::close(raw_fd);
  return result;
}

bool parse_silent(const std::string &s) {
  return s == "true" || s == "TRUE";
}

}  // namespace

int main(int argc, char **argv) {
  if (argc < 7) {
    std::fprintf(stderr,
                 "usage: %s connect <host> <port> <peer_pub> <send_file> <expect_file> <silent>\n"
                 "       %s accept  <host> <port>            <send_file> <expect_file> <silent>\n",
                 argv[0], argv[0]);
    return 64;
  }
  const std::string role = argv[1];
  const bool silent = parse_silent(argv[argc - 1]);

  if (role == "connect") {
    return run_connect(argv[2], std::atoi(argv[3]), argv[4], argv[5], argv[6],
                       silent);
  } else if (role == "accept") {
    return run_accept(argv[2], std::atoi(argv[3]), argv[4], argv[5], silent);
  } else {
    std::fprintf(stderr, "i2psam_runner: role must be connect or accept\n");
    return 64;
  }
}
