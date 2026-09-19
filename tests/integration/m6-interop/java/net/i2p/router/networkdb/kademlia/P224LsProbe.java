// Plan 224 WP B — test-only read-only LeaseSet snapshot probe.
//
// Compiled out-of-tree against the exact-pinned Java I2P 2.13.0 staged
// `lib/` jars into the ephemeral scratch build directory, exactly like
// `ControlledRouter.java`, `P222SelectorProbe.java`, and
// `P223BranchProbe.java`. Never compiled into or against the
// exact-pinned source checkout. It MUST NOT patch or replace any Java
// I2P class, MUST NOT mutate NetDB/KeyManager/tunnel/LeaseSet state,
// MUST NOT use reflection or private-field access, MUST NOT copy a
// LeaseSet into any facade, MUST NOT invoke any lookup API that would
// prime a client sub-DB (no `lookupLeaseSet` remote/search call — only
// the local read-only accessors below).
//
// Read-only public-API surface:
//   - main NetDB entry via
//     `KademliaNetworkDatabaseFacade.lookupLocallyWithoutValidation(target)`
//     (raw presence) and `lookupLeaseSetLocally(target)` (validated
//     presence) — the exact distinction Plan 224 §7.1 requires;
//   - client-subDB entry via the same two accessors on
//     `ctx.clientNetDb(clientDbid)` — the exact distinction §7.2
//     requires; a main-DB fallback (`isClientDb() == false`) is
//     reported explicitly and MUST NOT satisfy client authority;
//   - answerability flags via `DatabaseEntry.getReceivedAsPublished()`,
//     `getReceivedAsReply()`, `getReceivedBy()` (pinned
//     `HandleDatabaseLookupMessageJob` answers an LS DLM only from a
//     LeaseSet with `receivedAsPublished == true`);
//   - freshness via `LeaseSet.isCurrent(ctx.clock().now())`;
//   - bounded shape facts only: lease count, latest lease date,
//     LeaseSet2 key type codes and counts — never key bytes, never
//     session keys/tags, never payloads.
//
// All facts are bounded (key codes <= 8, sorted numeric) and carry no
// private or secret material.
package net.i2p.router.networkdb.kademlia;

import java.util.ArrayList;
import java.util.Collections;
import java.util.List;
import net.i2p.data.DatabaseEntry;
import net.i2p.data.Hash;
import net.i2p.data.LeaseSet;
import net.i2p.data.LeaseSet2;
import net.i2p.data.PublicKey;
import net.i2p.router.NetworkDatabaseFacade;
import net.i2p.router.RouterContext;

public final class P224LsProbe {

    private P224LsProbe() {
    }

    public static final class Result {
        /** True when `clientNetDb(clientDbid)` returned a facade (client variant). */
        public final boolean clientDbResolved;
        /** True when that facade `isClientDb()` (not a main fallback). */
        public final boolean clientDbIsClient;
        /** True when `lookupLocallyWithoutValidation(target)` is non-null. */
        public final boolean rawPresent;
        /** True when `lookupLeaseSetLocally(target)` is non-null. */
        public final boolean validatedPresent;
        /** `DatabaseEntry.getType()`, or -1 when absent/unreadable. */
        public final int entryType;
        /** `getReceivedAsPublished()` when an entry is present, else null (unknown). */
        public final Boolean receivedAsPublished;
        /** `getReceivedAsReply()` when an entry is present, else null (unknown). */
        public final Boolean receivedAsReply;
        /** Lowercase hex of `getReceivedBy()`, "none" when null, "unknown" when unreadable. */
        public final String receivedByHex;
        /** `LeaseSet2.isUnpublished()` when the entry is an LS2, else "na"/"unknown". */
        public final String ls2Unpublished;
        /** `LeaseSet.getLeaseCount()`, or -1 when the entry is not a LeaseSet. */
        public final int leaseCount;
        /** Bounded key count (LS2 list capped at 8; classic LS is 1), or -1 when not a LeaseSet. */
        public final int keyCount;
        /** Sorted numeric key type codes (capped at 8), possibly empty. */
        public final List<Integer> keyCodes;
        /** `LeaseSet.getLatestLeaseDate()`, or 0 when not a LeaseSet. */
        public final long latestLeaseMs;
        /** `LeaseSet.isCurrent(now)` when the entry is a LeaseSet, else null (unknown). */
        public final Boolean current;
        /** Machine-readable reason when the facade cannot be observed. Null on success. */
        public final String error;

        private Result(
                boolean clientDbResolved,
                boolean clientDbIsClient,
                boolean rawPresent,
                boolean validatedPresent,
                int entryType,
                Boolean receivedAsPublished,
                Boolean receivedAsReply,
                String receivedByHex,
                String ls2Unpublished,
                int leaseCount,
                int keyCount,
                List<Integer> keyCodes,
                long latestLeaseMs,
                Boolean current,
                String error) {
            this.clientDbResolved = clientDbResolved;
            this.clientDbIsClient = clientDbIsClient;
            this.rawPresent = rawPresent;
            this.validatedPresent = validatedPresent;
            this.entryType = entryType;
            this.receivedAsPublished = receivedAsPublished;
            this.receivedAsReply = receivedAsReply;
            this.receivedByHex = receivedByHex;
            this.ls2Unpublished = ls2Unpublished;
            this.leaseCount = leaseCount;
            this.keyCount = keyCount;
            this.keyCodes = keyCodes;
            this.latestLeaseMs = latestLeaseMs;
            this.current = current;
            this.error = error;
        }

        public static Result unavailable(String reason) {
            return new Result(false, false, false, false, -1, null, null,
                    "unknown", "unknown", -1, -1, new ArrayList<Integer>(),
                    0, null, reason);
        }

        public static Result clientUnresolved(String reason) {
            return new Result(false, false, false, false, -1, null, null,
                    "unknown", "unknown", -1, -1, new ArrayList<Integer>(),
                    0, null, reason);
        }

        public static Result clientFallbackToMain() {
            return new Result(true, false, false, false, -1, null, null,
                    "unknown", "unknown", -1, -1, new ArrayList<Integer>(),
                    0, null, "client-db-fallback-to-main");
        }
    }

    private static String hexLower(byte[] raw) {
        if (raw == null) {
            return "unknown";
        }
        StringBuilder out = new StringBuilder(raw.length * 2);
        for (byte b : raw) {
            out.append(String.format("%02x", b & 0xff));
        }
        return out.toString();
    }

    /**
     * Read-only snapshot of one target hash in a main NetDB facade.
     * Never stores, publishes, or searches; never mutates state.
     * Called by `ControlledRouter` for `P224-MAIN-LS`.
     */
    public static Result snapshotMain(
            RouterContext ctx, KademliaNetworkDatabaseFacade mainFacade, Hash targetHash) {
        if (ctx == null || mainFacade == null || targetHash == null) {
            return Result.unavailable("null-argument");
        }
        try {
            return snapshotFacade(ctx, mainFacade, targetHash, true, true);
        } catch (RuntimeException e) {
            return Result.unavailable("probe-threw-" + e.getClass().getSimpleName());
        }
    }

    /**
     * Read-only snapshot of one target hash in the helper client
     * sub-DB. Resolves `ctx.clientNetDb(clientDbid)`; when that
     * falls back to the main DB (`isClientDb() == false`) the result
     * is explicitly non-client and MUST NOT satisfy Plan-224 client
     * authority. Never stores, publishes, or searches; the two
     * accessors below perform local reads only and cannot prime the
     * sub-DB with a network lookup. Called by `ControlledRouter` for
     * `P224-CLIENT-LS`.
     */
    public static Result snapshotClient(
            RouterContext ctx, Hash clientDbid, Hash targetHash) {
        if (ctx == null || clientDbid == null || targetHash == null) {
            return Result.clientUnresolved("null-argument");
        }
        try {
            NetworkDatabaseFacade ndb = ctx.clientNetDb(clientDbid);
            if (ndb == null) {
                return Result.clientUnresolved("client-db-unresolved");
            }
            if (!(ndb instanceof KademliaNetworkDatabaseFacade)) {
                return Result.clientUnresolved("client-facade-not-kademlia");
            }
            KademliaNetworkDatabaseFacade clientFacade =
                    (KademliaNetworkDatabaseFacade) ndb;
            if (!clientFacade.isClientDb()) {
                return Result.clientFallbackToMain();
            }
            Result inner = snapshotFacade(ctx, clientFacade, targetHash, true, true);
            return new Result(true, true, inner.rawPresent,
                    inner.validatedPresent, inner.entryType,
                    inner.receivedAsPublished, inner.receivedAsReply,
                    inner.receivedByHex, inner.ls2Unpublished,
                    inner.leaseCount, inner.keyCount, inner.keyCodes,
                    inner.latestLeaseMs, inner.current, inner.error);
        } catch (RuntimeException e) {
            return Result.clientUnresolved("probe-threw-" + e.getClass().getSimpleName());
        }
    }

    private static Result snapshotFacade(
            RouterContext ctx, KademliaNetworkDatabaseFacade facade,
            Hash targetHash, boolean resolved, boolean isClient) {
        if (targetHash == null) {
            return new Result(resolved, isClient, false, false, -1, null,
                    null, "unknown", "unknown", -1, -1,
                    new ArrayList<Integer>(), 0, null, "null-target");
        }
        DatabaseEntry raw = null;
        try {
            raw = facade.lookupLocallyWithoutValidation(targetHash);
        } catch (RuntimeException re) {
            raw = null;
        }
        LeaseSet validated = null;
        try {
            validated = facade.lookupLeaseSetLocally(targetHash);
        } catch (RuntimeException re) {
            validated = null;
        }
        boolean rawPresent = (raw != null);
        boolean validatedPresent = (validated != null);
        if (!rawPresent && !validatedPresent) {
            return new Result(resolved, isClient, false, false, -1, null,
                    null, "none", "na", -1, -1,
                    new ArrayList<Integer>(), 0, null, null);
        }
        DatabaseEntry anchor = (raw != null) ? raw : validated;
        int entryType = -1;
        Boolean rap = null;
        Boolean rar = null;
        String byHex = "unknown";
        try {
            entryType = anchor.getType();
        } catch (RuntimeException re) {
            entryType = -1;
        }
        try {
            rap = anchor.getReceivedAsPublished() ? Boolean.TRUE : Boolean.FALSE;
        } catch (RuntimeException re) {
            rap = null;
        }
        try {
            rar = anchor.getReceivedAsReply() ? Boolean.TRUE : Boolean.FALSE;
        } catch (RuntimeException re) {
            rar = null;
        }
        try {
            Hash by = anchor.getReceivedBy();
            byHex = (by == null) ? "none" : hexLower(by.getData());
        } catch (RuntimeException re) {
            byHex = "unknown";
        }
        LeaseSet ls = (validated != null) ? validated
                : ((anchor instanceof LeaseSet) ? (LeaseSet) anchor : null);
        if (ls == null) {
            return new Result(resolved, isClient, rawPresent, validatedPresent,
                    entryType, rap, rar, byHex, "na", -1, -1,
                    new ArrayList<Integer>(), 0, null, null);
        }
        int leaseCount = -1;
        long latestMs = 0;
        try {
            leaseCount = ls.getLeaseCount();
        } catch (RuntimeException re) {
            leaseCount = -1;
        }
        try {
            latestMs = ls.getLatestLeaseDate();
        } catch (RuntimeException re) {
            latestMs = 0;
        }
        Boolean current = null;
        try {
            current = ls.isCurrent(ctx.clock().now()) ? Boolean.TRUE : Boolean.FALSE;
        } catch (RuntimeException re) {
            current = null;
        }
        if (ls instanceof LeaseSet2) {
            LeaseSet2 ls2 = (LeaseSet2) ls;
            String unpublished = "unknown";
            try {
                unpublished = ls2.isUnpublished() ? "true" : "false";
            } catch (RuntimeException re) {
                unpublished = "unknown";
            }
            List<Integer> codes = new ArrayList<Integer>();
            int count = 0;
            try {
                List<PublicKey> keys = ls2.getEncryptionKeys();
                if (keys != null) {
                    count = Math.min(keys.size(), 8);
                    for (int i = 0; i < count; i++) {
                        PublicKey k = keys.get(i);
                        if (k == null || k.getType() == null) {
                            continue;
                        }
                        codes.add(k.getType().getCode());
                    }
                    Collections.sort(codes);
                }
            } catch (RuntimeException re) {
                count = -1;
                codes = new ArrayList<Integer>();
            }
            return new Result(resolved, isClient, rawPresent, validatedPresent,
                    entryType, rap, rar, byHex, unpublished, leaseCount,
                    count, codes, latestMs, current, null);
        }
        int singleCode = -1;
        try {
            PublicKey single = ls.getEncryptionKey();
            if (single != null && single.getType() != null) {
                singleCode = single.getType().getCode();
            }
        } catch (RuntimeException re) {
            singleCode = -1;
        }
        List<Integer> codes = new ArrayList<Integer>();
        if (singleCode >= 0) {
            codes.add(singleCode);
        }
        return new Result(resolved, isClient, rawPresent, validatedPresent,
                entryType, rap, rar, byHex, "na", leaseCount,
                (singleCode >= 0 ? 1 : -1), codes, latestMs, current, null);
    }
}

