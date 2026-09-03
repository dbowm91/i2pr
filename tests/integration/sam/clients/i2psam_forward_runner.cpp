// SPDX-License-Identifier: MIT
//
// Plan 150 §11 — STREAM FORWARD i2psam registerer.

#include <cstdio>
#include <cstdlib>
#include <string>
#include <unistd.h>

#include "i2psam.h"
#include "i2p_base64.h"

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
  if (session.isSick()) {
    std::fprintf(stderr, "i2psam_forward_runner: SESSION CREATE rejected\n");
    return 3;
  }

  // i2psam stores the full private destination in FullDestination::pub
  // because that is the value returned by SESSION STATUS.  The SAM
  // STREAM CONNECT target is the first 391 public bytes, encoded with
  // the canonical two padding characters.
  const auto &dest = session.getMyDestination();
  const std::string pub = plan150::public_from_private(dest.pub);
  if (pub.size() != 524) {
    std::fprintf(stderr, "i2psam_forward_runner: public destination projection failed\n");
    return 3;
  }
  std::fprintf(stdout, "%s\n", pub.c_str());
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
