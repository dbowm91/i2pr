// SPDX-License-Identifier: MIT
//
// Plan 150 §11 — STREAM FORWARD i2psam registerer.

#include <cstdio>
#include <cstdlib>
#include <string>

#include "i2psam.h"

int main(int argc, char **argv) {
  if (argc != 5) {
    std::fprintf(stderr,
                 "usage: %s <host> <port> <loopback_host> <loopback_port>\n",
                 argv[0]);
    return 64;
  }
  const std::string host = argv[1];
  const int port = std::atoi(argv[2]);
  const std::string fwd_host = argv[3];
  const int fwd_port = std::atoi(argv[4]);

  SAM::StreamSession session("i2psam-forward-runner", host,
                              static_cast<uint16_t>(port), "TRANSIENT");

  // The FullDestination is the local destination the SAM bridge
  // // emitted in the SESSION STATUS reply.
  const auto &dest = session.getMyDestination();
  std::fprintf(stdout, "%s\n", dest.pub.c_str());
  std::fflush(stdout);

  auto result = session.forward(fwd_host, static_cast<uint16_t>(fwd_port),
                                /*silent=*/false);
  if (!result.isOk) {
    std::fprintf(stderr, "i2psam_forward_runner: STREAM FORWARD failed\n");
    return 4;
  }
  while (true) {
    sleep(60);
  }
  return 0;
}
