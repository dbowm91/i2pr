// i2pd Red25519 blinding differential oracle (Plan 331).
//
// Read-only use of the pinned i2pd reference at 2c694149fa6996eaeb23e378d5f83c9d3232c22f.
// This program links the reference library as an unmodified dependency and prints
// independently derived values for one blinded-key case. It exists only to produce expected
// values for the i2pr differential; no i2pd source is copied into i2pr, and no production
// behavior is derived from this file.
//
// Usage: i2pd-red25519-oracle <sigtype 7|11> <unblinded pubkey hex> <unblinded privkey hex> <YYYYMMDD>
// Output: one "key=value" line per derived value, all hex.

#include <cstdio>
#include <cstring>
#include <memory>
#include <string>

#include "Blinding.h"
#include "Crypto.h"
#include "I2PEndian.h"
#include "Identity.h"
#include "Signature.h"

using namespace i2p::data;

namespace {

int from_hex (const char* text, uint8_t* out, size_t len)
{
	size_t written = 0;
	for (size_t i = 0; text[i * 2] && text[i * 2 + 1]; i++)
	{
		if (i >= len) return 0;
		char pair[3] = {text[i * 2], text[i * 2 + 1], 0};
		out[i] = (uint8_t)strtoul (pair, nullptr, 16);
		written++;
	}
	return written == len;
}

void print_hex (const char* label, const uint8_t* data, size_t len)
{
	printf ("%s=", label);
	for (size_t i = 0; i < len; i++) printf ("%02x", data[i]);
	printf ("\n");
}

} // namespace

int main (int argc, char** argv)
{
	if (argc != 5)
	{
		fprintf (stderr, "usage: %s <sigtype 7|11> <pubkey hex> <privkey hex> <YYYYMMDD>\n", argv[0]);
		return 2;
	}
	const uint16_t sigtype = (uint16_t)atoi (argv[1]);
	if (sigtype != SIGNING_KEY_TYPE_EDDSA_SHA512_ED25519 &&
		sigtype != SIGNING_KEY_TYPE_REDDSA_SHA512_ED25519)
	{
		fprintf (stderr, "unsupported sigtype\n");
		return 2;
	}

	uint8_t publicKey[i2p::crypto::EDDSA25519_PUBLIC_KEY_LENGTH];
	uint8_t privateKey[i2p::crypto::EDDSA25519_PRIVATE_KEY_LENGTH];
	uint8_t encryptionKey[i2p::crypto::EDDSA25519_PUBLIC_KEY_LENGTH];
	if (!from_hex (argv[2], publicKey, sizeof (publicKey)) ||
		!from_hex (argv[3], privateKey, sizeof (privateKey)))
	{
		fprintf (stderr, "malformed hex input\n");
		return 2;
	}
	memset (encryptionKey, 0, sizeof (encryptionKey));
	const std::string date (argv[4]);
	if (date.size () != 8)
	{
		fprintf (stderr, "date must be 8 characters\n");
		return 2;
	}

	// IdentityEx takes (encryption public key, signing public key, ...); blinding only reads
	// the signing public key, so the encryption key is an unused placeholder.
	auto identity = std::make_shared<IdentityEx> (
		encryptionKey, publicKey, sigtype, CRYPTO_KEY_TYPE_ECIES_X25519_AEAD);
	BlindedPublicKey blinded (identity);

	uint8_t blindedPublic[i2p::crypto::EDDSA25519_PUBLIC_KEY_LENGTH];
	uint8_t blindedPrivate[i2p::crypto::EDDSA25519_PRIVATE_KEY_LENGTH];
	uint8_t blindedPublic2[i2p::crypto::EDDSA25519_PUBLIC_KEY_LENGTH];
	const size_t blindedLength = blinded.BlindPrivateKey (privateKey, date.c_str (), blindedPrivate, blindedPublic);
	if (!blindedLength)
	{
		fprintf (stderr, "blinding failed\n");
		return 1;
	}
	const size_t publicLength = blinded.GetBlindedKey (date.c_str (), blindedPublic2);
	if (publicLength != blindedLength || memcmp (blindedPublic, blindedPublic2, blindedLength))
	{
		fprintf (stderr, "private and public blinding disagree\n");
		return 1;
	}

	// Sign a fixed message with the blinded key and verify it, so the oracle also proves
	// signature acceptance rather than only byte agreement.
	const uint8_t message[] = "i2pr plan 331 differential";
	std::unique_ptr<i2p::crypto::Signer> signer (
		PrivateKeys::CreateSigner (blinded.GetBlindedSigType (), blindedPrivate));
	uint8_t signature[i2p::crypto::EDDSA25519_SIGNATURE_LENGTH];
	signer->Sign (message, sizeof (message), signature);
	std::unique_ptr<i2p::crypto::Verifier> verifier (
		IdentityEx::CreateVerifier (blinded.GetBlindedSigType ()));
	verifier->SetPublicKey (blindedPublic);
	if (!verifier->Verify (message, sizeof (message), signature))
	{
		fprintf (stderr, "blinded signature did not verify\n");
		return 1;
	}

	auto storeHash = blinded.GetStoreHash (date.c_str ());
	print_hex ("unblinded_public", publicKey, sizeof (publicKey));
	print_hex ("blinded_public", blindedPublic, blindedLength);
	print_hex ("blinded_private", blindedPrivate, sizeof (blindedPrivate));
	print_hex ("storage_key", storeHash.data (), i2p::crypto::EDDSA25519_PUBLIC_KEY_LENGTH);
	printf ("b33=%s\n", blinded.ToB33 ().c_str ());
	printf ("blinded_sigtype=%u\n", (unsigned)blinded.GetBlindedSigType ());
	// Emitted so the i2pr implementation can verify a signature produced by this reference.
	print_hex ("message", message, sizeof (message));
	print_hex ("signature", signature, sizeof (signature));
	printf ("signature_verified=true\n");
	return 0;
}
