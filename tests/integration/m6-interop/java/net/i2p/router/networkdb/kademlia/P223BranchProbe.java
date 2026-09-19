// Plan 223 WP C — test-only bounded status-17 branch discriminator.
//
// Compiled out-of-tree against the exact-pinned Java I2P 2.13.0 staged
// `lib/` jars into the ephemeral scratch build directory, exactly like
// `ControlledRouter.java` and `P222SelectorProbe.java`. Never compiled
// into or against the exact-pinned source checkout. It MUST NOT patch
// or replace any Java I2P class, MUST NOT mutate NetDB/KeyManager/tunnel
// state, MUST NOT use reflection or private-field access.
//
// Read-only public-API surface:
//   - source helper LeaseSetKeys via `RouterContext.keyManager().getKeys(Hash)`
//     + `LeaseSetKeys.getSupportedEncryption()`;
//   - target LS2 as Java stores it via
//     `clientNetDb(clientDbid).lookupLeaseSetLocally(targetHash)`;
//   - exact compatibility intersection via
//     `LeaseSet2.getEncryptionKey(supported)` with the pinned OCMOSJ
//     fallback (`LeaseSetKeys.SET_ELG` when source keys are absent).
//
// All facts are bounded (key counts <= 8, type sets ordered numeric) and
// carry no private key material.
package net.i2p.router.networkdb.kademlia;

import java.util.ArrayList;
import java.util.Collections;
import java.util.List;
import java.util.Set;
import net.i2p.crypto.EncType;
import net.i2p.data.Destination;
import net.i2p.data.Hash;
import net.i2p.data.LeaseSet;
import net.i2p.data.LeaseSet2;
import net.i2p.data.PublicKey;
import net.i2p.router.LeaseSetKeys;
import net.i2p.router.NetworkDatabaseFacade;
import net.i2p.router.RouterContext;

public final class P223BranchProbe {

    private P223BranchProbe() {
    }

    public static final class Result {
        public final boolean sourceKeysPresent;
        public final List<Integer> sourceSupportedCodes;
        public final boolean sourceSupportsElgamal;
        public final boolean sourceSupportsX25519;
        public final boolean targetLsPresent;
        public final String targetLsType;
        public final boolean targetDestinationHashMatch;
        public final int targetDestinationEncType;
        public final int targetKeyCount;
        public final List<Integer> targetKeyCodes;
        public final boolean targetHasX25519;
        public final boolean selectedKeyPresent;
        public final int selectedKeyType;
        public final String error;

        private Result(
                boolean sourceKeysPresent,
                List<Integer> sourceSupportedCodes,
                boolean sourceSupportsElgamal,
                boolean sourceSupportsX25519,
                boolean targetLsPresent,
                String targetLsType,
                boolean targetDestinationHashMatch,
                int targetDestinationEncType,
                int targetKeyCount,
                List<Integer> targetKeyCodes,
                boolean targetHasX25519,
                boolean selectedKeyPresent,
                int selectedKeyType,
                String error) {
            this.sourceKeysPresent = sourceKeysPresent;
            this.sourceSupportedCodes = sourceSupportedCodes;
            this.sourceSupportsElgamal = sourceSupportsElgamal;
            this.sourceSupportsX25519 = sourceSupportsX25519;
            this.targetLsPresent = targetLsPresent;
            this.targetLsType = targetLsType;
            this.targetDestinationHashMatch = targetDestinationHashMatch;
            this.targetDestinationEncType = targetDestinationEncType;
            this.targetKeyCount = targetKeyCount;
            this.targetKeyCodes = targetKeyCodes;
            this.targetHasX25519 = targetHasX25519;
            this.selectedKeyPresent = selectedKeyPresent;
            this.selectedKeyType = selectedKeyType;
            this.error = error;
        }

        public static Result unavailable(String reason) {
            return new Result(false, new ArrayList<Integer>(), false, false,
                    false, "none", false, -1, 0, new ArrayList<Integer>(),
                    false, false, -1, reason);
        }
    }

    private static List<Integer> orderedCodes(Set<EncType> types) {
        List<Integer> out = new ArrayList<Integer>();
        if (types == null) {
            return out;
        }
        for (EncType t : types) {
            if (t == null) {
                continue;
            }
            out.add(t.getCode());
            if (out.size() >= 8) {
                break;
            }
        }
        Collections.sort(out);
        return out;
    }

    /**
     * Bounded read-only branch discriminator for one client facade.
     * Never registers keys, installs LeaseSets, or alters the client DB.
     */
    public static Result branchForClient(
            RouterContext ctx, Hash clientDbid, Hash targetHash, Hash sourceHash) {
        if (ctx == null || clientDbid == null || targetHash == null || sourceHash == null) {
            return Result.unavailable("null-argument");
        }
        try {
            NetworkDatabaseFacade ndb = ctx.clientNetDb(clientDbid);
            if (ndb == null) {
                return Result.unavailable("client-db-unresolved");
            }
            if (!(ndb instanceof KademliaNetworkDatabaseFacade)) {
                return Result.unavailable("client-facade-not-kademlia");
            }
            KademliaNetworkDatabaseFacade clientFacade =
                    (KademliaNetworkDatabaseFacade) ndb;

            // C1: source helper LeaseSetKeys (read-only).
            LeaseSetKeys keys = null;
            try {
                keys = ctx.keyManager().getKeys(sourceHash);
            } catch (RuntimeException re) {
                keys = null;
            }
            boolean sourcePresent = (keys != null);
            Set<EncType> supported = null;
            if (sourcePresent) {
                try {
                    supported = keys.getSupportedEncryption();
                } catch (RuntimeException re) {
                    supported = null;
                }
            }
            List<Integer> sourceCodes = orderedCodes(supported);
            boolean supportsElgamal = false;
            boolean supportsX25519 = false;
            if (supported != null) {
                for (EncType t : supported) {
                    if (t == null) {
                        continue;
                    }
                    if (t.getCode() == 0) {
                        supportsElgamal = true;
                    }
                    if (t.getCode() == 4) {
                        supportsX25519 = true;
                    }
                }
            }

            // C2: target LS2 as Java stores it (read-only local lookup).
            LeaseSet ls = null;
            try {
                ls = clientFacade.lookupLeaseSetLocally(targetHash);
            } catch (RuntimeException re) {
                ls = null;
            }
            if (ls == null) {
                // No target LS: intersection is unknowable; selected is absent.
                return new Result(sourcePresent, sourceCodes, supportsElgamal,
                        supportsX25519, false, "none", false, -1, 0,
                        new ArrayList<Integer>(), false, false, -1, null);
            }
            String lsType = ls.getClass().getSimpleName();
            if (!(ls instanceof LeaseSet2)) {
                // Classic LeaseSet path (not expected for type-3 lane):
                // record destination enc type but no X25519 key set.
                int destEnc = -1;
                boolean hashMatch = false;
                try {
                    Destination d = ls.getDestination();
                    if (d != null) {
                        destEnc = d.getEncType().getCode();
                        hashMatch = d.calculateHash().equals(targetHash);
                    }
                } catch (RuntimeException re) {
                    destEnc = -1;
                }
                return new Result(sourcePresent, sourceCodes, supportsElgamal,
                        supportsX25519, true, lsType, hashMatch, destEnc, 0,
                        new ArrayList<Integer>(), false, false, -1, null);
            }
            LeaseSet2 ls2 = (LeaseSet2) ls;
            boolean hashMatch = false;
            int destEnc = -1;
            try {
                Destination d = ls2.getDestination();
                if (d != null) {
                    destEnc = d.getEncType().getCode();
                    hashMatch = d.calculateHash().equals(targetHash);
                }
            } catch (RuntimeException re) {
                destEnc = -1;
            }
            List<PublicKey> encKeys = null;
            try {
                encKeys = ls2.getEncryptionKeys();
            } catch (RuntimeException re) {
                encKeys = null;
            }
            List<Integer> keyCodes = new ArrayList<Integer>();
            boolean hasX25519 = false;
            int keyCount = 0;
            if (encKeys != null) {
                keyCount = Math.min(encKeys.size(), 8);
                for (int i = 0; i < keyCount; i++) {
                    PublicKey k = encKeys.get(i);
                    if (k == null || k.getType() == null) {
                        continue;
                    }
                    int code = k.getType().getCode();
                    keyCodes.add(code);
                    if (code == 4) {
                        hasX25519 = true;
                    }
                }
                Collections.sort(keyCodes);
            }

            // C3: exact Java compatibility intersection (read-only).
            // Pinned OCMOSJ: supported = ourKeys.getSupportedEncryption()
            // else SET_ELG; selected = leaseSet.getEncryptionKey(supported).
            Set<EncType> forSelection = supported;
            if (!sourcePresent || forSelection == null) {
                forSelection = LeaseSetKeys.SET_ELG;
            }
            PublicKey selected = null;
            try {
                selected = ls2.getEncryptionKey(forSelection);
            } catch (RuntimeException re) {
                selected = null;
            }
            boolean selectedPresent = (selected != null);
            int selectedType = -1;
            if (selectedPresent && selected.getType() != null) {
                selectedType = selected.getType().getCode();
            }
            return new Result(sourcePresent, sourceCodes, supportsElgamal,
                    supportsX25519, true, lsType, hashMatch, destEnc,
                    keyCount, keyCodes, hasX25519, selectedPresent,
                    selectedType, null);
        } catch (RuntimeException e) {
            return Result.unavailable("probe-threw-" + e.getClass().getSimpleName());
        }
    }

    /**
     * Read-only Destination parse for exact-byte cross-checks.
     * Returns a single-line fact string; never mutates state.
     */
    public static String inspectDestination(String b64) {
        if (b64 == null || b64.isEmpty() || b64.length() > 2048) {
            return "P223-EV kind=dest-inspect observable=false reason=invalid-argument";
        }
        try {
            Destination peer = new Destination(b64);
            Hash h = peer.calculateHash();
            StringBuilder hex = new StringBuilder(h.getData().length * 2);
            for (byte b : h.getData()) {
                hex.append(String.format("%02x", b & 0xff));
            }
            int encCode = peer.getEncType().getCode();
            String encName = peer.getEncType().name();
            int pubLen = peer.getPublicKey().length();
            int sigCode = peer.getSigType().getCode();
            return "P223-EV kind=dest-inspect observable=true"
                + " hash_hex=" + hex.toString()
                + " enc_type_code=" + encCode
                + " enc_type_name=" + encName
                + " public_key_len=" + pubLen
                + " sig_type_code=" + sigCode;
        } catch (Throwable t) {
            return "P223-EV kind=dest-inspect observable=false reason="
                + t.getClass().getSimpleName();
        }
    }
}
