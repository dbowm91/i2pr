// SPDX-License-Identifier: MIT
// Small harness-only I2P Base64 projection used to compare a full PRIV with
// the public Destination returned by NAMING. It is not a SAM implementation.

#ifndef I2PR_PLAN150_I2P_BASE64_H
#define I2PR_PLAN150_I2P_BASE64_H

#include <cstddef>
#include <cstdint>
#include <string>
#include <vector>

namespace plan150 {

inline int i2p_base64_value(unsigned char value) {
  if (value >= 'A' && value <= 'Z') return value - 'A';
  if (value >= 'a' && value <= 'z') return value - 'a' + 26;
  if (value >= '0' && value <= '9') return value - '0' + 52;
  if (value == '-') return 62;
  if (value == '~') return 63;
  return -1;
}

inline bool i2p_base64_decode(const std::string &input,
                              std::vector<unsigned char> &output) {
  output.clear();
  uint32_t accumulator = 0;
  unsigned bits = 0;
  for (unsigned char value : input) {
    if (value == '=') break;
    const int digit = i2p_base64_value(value);
    if (digit < 0) return false;
    accumulator = (accumulator << 6) | static_cast<uint32_t>(digit);
    bits += 6;
    if (bits >= 8) {
      bits -= 8;
      output.push_back(static_cast<unsigned char>((accumulator >> bits) & 0xff));
    }
  }
  return true;
}

inline std::string i2p_base64_encode(const std::vector<unsigned char> &input) {
  static constexpr char alphabet[] =
      "ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789-~";
  std::string output;
  output.reserve(((input.size() + 2) / 3) * 4);
  for (std::size_t offset = 0; offset < input.size(); offset += 3) {
    const std::size_t remaining = input.size() - offset;
    const uint32_t first = input[offset];
    const uint32_t second = remaining > 1 ? input[offset + 1] : 0;
    const uint32_t third = remaining > 2 ? input[offset + 2] : 0;
    const uint32_t block = (first << 16) | (second << 8) | third;
    output.push_back(alphabet[(block >> 18) & 0x3f]);
    output.push_back(alphabet[(block >> 12) & 0x3f]);
    output.push_back(remaining > 1 ? alphabet[(block >> 6) & 0x3f] : '=');
    output.push_back(remaining > 2 ? alphabet[block & 0x3f] : '=');
  }
  return output;
}

inline std::string public_from_private(const std::string &private_destination) {
  std::vector<unsigned char> decoded;
  if (!i2p_base64_decode(private_destination, decoded) || decoded.size() < 391) {
    return {};
  }
  decoded.resize(391);
  return i2p_base64_encode(decoded);
}

}  // namespace plan150

#endif
