/* SPDX-License-Identifier: MIT
 *
 * Plan 150 §6 — libsam3 runner.
 *
 * Drives the i2pr SAM 3.1 listener through libsam3 (snapshot
 * 7d6e658798baec31394c5685f9583343cc00900b) without linking any
 * i2pr crate. This file is harness code; it intentionally calls
 * libsam3's normal public API only.
 *
 * Usage:
 *   libsam3_runner connect <sam_host> <sam_port> <peer_pub>
 *                        <send_payload_file> <expect_payload_file> <silent:true|false>
 *   libsam3_runner accept <sam_host> <sam_port>
 *                        <send_payload_file> <expect_payload_file> <silent:true|false>
 *
 * On the ACCEPT side the runner:
 *   1. opens a STREAM session and calls STREAM ACCEPT;
 *   2. reads exactly `expect_len` bytes (the peer will send them);
 *   3. compares them against `expect_payload_file`;
 *   4. writes the contents of `send_payload_file` back as the echo;
 *   5. exits 0 on success.
 *
 * On the CONNECT side the runner:
 *   1. opens a STREAM session and calls STREAM CONNECT to <peer_pub>;
 *   2. writes the contents of `send_payload_file` to the stream;
 *   3. reads exactly `expect_len` bytes from the stream;
 *   4. compares them against `expect_payload_file`;
 *   5. exits 0 on success.
 *
 * Both modes accept `silent:true|false` to exercise the SAM 3.1
 * CONNECT SILENT path. libsam3's silent flag is session-wide
 * (sam3CreateSilentSession) so the harness always uses the
 * session-level mode that maps to the requested option.
 */

#include <errno.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <sys/socket.h>
#include <unistd.h>

#include "libsam3.h"

static int read_file(const char *path, unsigned char **out_buf, size_t *out_len) {
  FILE *fp = fopen(path, "rb");
  if (fp == NULL) {
    fprintf(stderr, "libsam3_runner: cannot open %s: %s\n", path, strerror(errno));
    return -1;
  }
  if (fseek(fp, 0, SEEK_END) != 0) {
    fclose(fp);
    return -1;
  }
  long size = ftell(fp);
  if (size < 0) {
    fclose(fp);
    return -1;
  }
  rewind(fp);
  unsigned char *buf = (unsigned char *)malloc((size_t)size);
  if (buf == NULL) {
    fclose(fp);
    return -1;
  }
  size_t got = fread(buf, 1, (size_t)size, fp);
  fclose(fp);
  if (got != (size_t)size) {
    free(buf);
    return -1;
  }
  *out_buf = buf;
  *out_len = (size_t)size;
  return 0;
}

static int compare_bytes(const unsigned char *got, size_t got_len,
                          const unsigned char *want, size_t want_len) {
  if (got_len != want_len) {
    fprintf(stderr, "libsam3_runner: length mismatch got=%zu want=%zu\n",
            got_len, want_len);
    return -1;
  }
  if (memcmp(got, want, got_len) != 0) {
    fprintf(stderr, "libsam3_runner: payload content mismatch\n");
    return -1;
  }
  return 0;
}

static int run_connect(const char *host, int port, const char *peer_pub,
                       const char *send_path, const char *expect_path,
                       int silent) {
  unsigned char *send_buf = NULL;
  unsigned char *expect_buf = NULL;
  size_t send_len = 0;
  size_t expect_len = 0;
  if (read_file(send_path, &send_buf, &send_len) != 0) return 2;
  if (read_file(expect_path, &expect_buf, &expect_len) != 0) { free(send_buf); return 2; }

  Sam3Session ses;
  memset(&ses, 0, sizeof(ses));
  int rc;
  if (silent) {
    rc = sam3CreateSilentSession(&ses, host, port, SAM3_DESTINATION_TRANSIENT,
                                  SAM3_SESSION_STREAM, EdDSA_SHA512_Ed25519, NULL);
  } else {
    rc = sam3CreateSession(&ses, host, port, SAM3_DESTINATION_TRANSIENT,
                            SAM3_SESSION_STREAM, EdDSA_SHA512_Ed25519, NULL);
  }
  if (rc < 0) {
    fprintf(stderr, "libsam3_runner: SESSION CREATE failed\n");
    free(send_buf);
    free(expect_buf);
    return 3;
  }

  Sam3Connection *conn = sam3StreamConnect(&ses, peer_pub);
  if (conn == NULL) {
    fprintf(stderr, "libsam3_runner: STREAM CONNECT failed: %s\n", ses.error);
    sam3CloseSession(&ses);
    free(send_buf);
    free(expect_buf);
    return 4;
  }

  int fd = conn->fd;
  if (sam3tcpSend(fd, send_buf, send_len) < 0) {
    fprintf(stderr, "libsam3_runner: send failed\n");
    sam3CloseConnection(conn);
    sam3CloseSession(&ses);
    free(send_buf);
    free(expect_buf);
    return 5;
  }
  /* Keep the stream open while the peer sends its delayed echo. */

  unsigned char *recv_buf = (unsigned char *)malloc(expect_len);
  if (recv_buf == NULL) {
    sam3CloseConnection(conn);
    sam3CloseSession(&ses);
    free(send_buf);
    free(expect_buf);
    return 6;
  }
  size_t total = 0;
  while (total < expect_len) {
    ssize_t n = sam3tcpReceive(fd, recv_buf + total, expect_len - total);
    if (n <= 0) {
      if (n == 0 && total == expect_len) break;
      fprintf(stderr, "libsam3_runner: short read total=%zu want=%zu n=%zd\n",
              total, expect_len, n);
      free(recv_buf);
      sam3CloseConnection(conn);
      sam3CloseSession(&ses);
      free(send_buf);
      free(expect_buf);
      return 7;
    }
    total += (size_t)n;
  }

  int result = 0;
  if (compare_bytes(recv_buf, total, expect_buf, expect_len) != 0) result = 8;

  free(recv_buf);
  sam3CloseConnection(conn);
  sam3CloseSession(&ses);
  free(send_buf);
  free(expect_buf);
  return result;
}

static int run_accept(const char *host, int port, const char *send_path,
                       const char *expect_path, int silent) {
  unsigned char *send_buf = NULL;
  unsigned char *expect_buf = NULL;
  size_t send_len = 0;
  size_t expect_len = 0;
  if (read_file(send_path, &send_buf, &send_len) != 0) return 2;
  if (read_file(expect_path, &expect_buf, &expect_len) != 0) { free(send_buf); return 2; }

  Sam3Session ses;
  memset(&ses, 0, sizeof(ses));
  int rc;
  if (silent) {
    rc = sam3CreateSilentSession(&ses, host, port, SAM3_DESTINATION_TRANSIENT,
                                  SAM3_SESSION_STREAM, EdDSA_SHA512_Ed25519, NULL);
  } else {
    rc = sam3CreateSession(&ses, host, port, SAM3_DESTINATION_TRANSIENT,
                            SAM3_SESSION_STREAM, EdDSA_SHA512_Ed25519, NULL);
  }
  if (rc < 0) {
    fprintf(stderr, "libsam3_runner: SESSION CREATE failed\n");
    free(send_buf);
    free(expect_buf);
    return 3;
  }

  Sam3Connection *conn = sam3StreamAccept(&ses);
  if (conn == NULL) {
    fprintf(stderr, "libsam3_runner: STREAM ACCEPT failed: %s\n", ses.error);
    sam3CloseSession(&ses);
    free(send_buf);
    free(expect_buf);
    return 4;
  }

  int fd = conn->fd;
  unsigned char *recv_buf = (unsigned char *)malloc(expect_len);
  if (recv_buf == NULL) {
    sam3CloseConnection(conn);
    sam3CloseSession(&ses);
    free(send_buf);
    free(expect_buf);
    return 6;
  }
  size_t total = 0;
  while (total < expect_len) {
    ssize_t n = sam3tcpReceive(fd, recv_buf + total, expect_len - total);
    if (n <= 0) {
      fprintf(stderr, "libsam3_runner: short accept read total=%zu want=%zu n=%zd\n",
              total, expect_len, n);
      free(recv_buf);
      sam3CloseConnection(conn);
      sam3CloseSession(&ses);
      free(send_buf);
      free(expect_buf);
      return 7;
    }
    total += (size_t)n;
  }

  int result = 0;
  if (compare_bytes(recv_buf, total, expect_buf, expect_len) != 0) result = 8;

  if (sam3tcpSend(fd, send_buf, send_len) < 0) {
    fprintf(stderr, "libsam3_runner: accept echo failed\n");
    result = 9;
  }
  free(recv_buf);
  sam3CloseConnection(conn);
  sam3CloseSession(&ses);
  free(send_buf);
  free(expect_buf);
  return result;
}

static int parse_silent(const char *s) {
  if (strcmp(s, "true") == 0 || strcmp(s, "TRUE") == 0) return 1;
  if (strcmp(s, "false") == 0 || strcmp(s, "FALSE") == 0) return 0;
  return -1;
}

int main(int argc, char **argv) {
  if (argc < 8) {
    fprintf(stderr,
            "usage: %s connect <host> <port> <peer_pub> <send_file> <expect_file> <silent>\n"
            "       %s accept  <host> <port>            <send_file> <expect_file> <silent>\n",
            argv[0], argv[0]);
    return 64;
  }
  const char *role = argv[1];
  int silent = parse_silent(argv[argc - 1]);
  if (silent < 0) {
    fprintf(stderr, "libsam3_runner: invalid silent flag\n");
    return 64;
  }

  if (strcmp(role, "connect") == 0) {
    const char *host = argv[2];
    int port = atoi(argv[3]);
    const char *peer_pub = argv[4];
    const char *send_path = argv[5];
    const char *expect_path = argv[6];
    return run_connect(host, port, peer_pub, send_path, expect_path, silent);
  } else if (strcmp(role, "accept") == 0) {
    const char *host = argv[2];
    int port = atoi(argv[3]);
    const char *send_path = argv[4];
    const char *expect_path = argv[5];
    return run_accept(host, port, send_path, expect_path, silent);
  } else {
    fprintf(stderr, "libsam3_runner: role must be connect or accept\n");
    return 64;
  }
}
