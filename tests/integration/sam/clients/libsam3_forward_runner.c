/* SPDX-License-Identifier: MIT
 *
 * Plan 150 §11 — STREAM FORWARD libsam3 registerer.
 *
 * Creates a non-transient STREAM session through libsam3 (snapshot
 * 7d6e658798baec31394c5685f9583343cc00900b), exports the session
 * public key (PUB) on stdout, and registers STREAM FORWARD to the
 * supplied loopback target. The harness then drives a CONNECT from
 * the second external client (or the Python transcript) through
 * the i2pr SAM listener.
 */

#include <errno.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <unistd.h>

#include "libsam3.h"

int main(int argc, char **argv) {
  if (argc != 5) {
    fprintf(stderr,
            "usage: %s <host> <port> <loopback_host> <loopback_port>\n",
            argv[0]);
    return 64;
  }
  const char *host = argv[1];
  int port = atoi(argv[2]);
  const char *fwd_host = argv[3];
  int fwd_port = atoi(argv[4]);

  Sam3Session ses;
  memset(&ses, 0, sizeof(ses));
  if (sam3CreateSession(&ses, host, port, SAM3_DESTINATION_TRANSIENT,
                        SAM3_SESSION_STREAM, EdDSA_SHA512_Ed25519,
                        NULL) < 0) {
    fprintf(stderr, "libsam3_forward_runner: SESSION CREATE failed\n");
    return 3;
  }
  fprintf(stdout, "%s\n", ses.pubkey);
  fflush(stdout);

  if (sam3StreamForward(&ses, fwd_host, fwd_port) < 0) {
    fprintf(stderr, "libsam3_forward_runner: STREAM FORWARD failed: %s\n",
            ses.error);
    sam3CloseSession(&ses);
    return 4;
  }
  // Block until the harness cancels us. libsam3 keeps fwd_fd open
  // for the lifetime of the FORWARD registration.
  for (;;) {
    sleep(60);
  }
  return 0;
}
