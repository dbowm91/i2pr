// Plan 470 read-only Java selector and NTCP transport bid preflight.
// Compiled out-of-tree against the exact pinned stock jars. It calls the
// stock floodfill selector and transport bid APIs; it does not send I2NP.
package net.i2p.router.networkdb.kademlia;

import java.util.SortedMap;
import java.lang.reflect.Field;
import java.util.concurrent.atomic.AtomicInteger;
import net.i2p.data.Hash;
import net.i2p.data.DatabaseEntry;
import net.i2p.data.i2np.DatabaseStoreMessage;
import net.i2p.data.router.RouterInfo;
import net.i2p.router.Job;
import net.i2p.router.JobImpl;
import net.i2p.router.RouterContext;
import net.i2p.stat.RateStat;
import net.i2p.router.transport.Transport;
import net.i2p.router.transport.TransportBid;
import net.i2p.router.transport.ntcp.NTCPTransport;

public final class P470TransportPreflight {
    private static final AtomicInteger STORE_SUBMITTED = new AtomicInteger();
    private static final AtomicInteger STORE_SUCCEEDED = new AtomicInteger();
    private static final AtomicInteger STORE_FAILED = new AtomicInteger();

    public static final class Result {
        public final boolean candidatePresent;
        public final boolean routerHidden;
        public final boolean floodfillEnabled;
        public final boolean acceptsRouterInfoStore;
        public final boolean selectedForLocalRouterInfo;
        public final boolean candidateBanlisted;
        public final boolean ntcpTransportUnreachable;
        public final boolean ntcp2TransportAvailable;
        public final boolean ntcp2BidAvailable;
        public final boolean bidUsesNtcpTransport;
        public final int bidLatencyMs;
        public final int actualStoreMessageSize;
        public final boolean actualFloodSelectorIncludesCandidate;
        public final String error;

        private Result(boolean candidatePresent, boolean routerHidden,
                       boolean floodfillEnabled,
                       boolean acceptsRouterInfoStore,
                       boolean selectedForLocalRouterInfo,
                       boolean candidateBanlisted, boolean ntcpTransportUnreachable,
                       boolean ntcp2TransportAvailable, boolean ntcp2BidAvailable,
                       boolean bidUsesNtcpTransport, int bidLatencyMs,
                       int actualStoreMessageSize,
                       boolean actualFloodSelectorIncludesCandidate, String error) {
            this.candidatePresent = candidatePresent;
            this.routerHidden = routerHidden;
            this.floodfillEnabled = floodfillEnabled;
            this.acceptsRouterInfoStore = acceptsRouterInfoStore;
            this.selectedForLocalRouterInfo = selectedForLocalRouterInfo;
            this.candidateBanlisted = candidateBanlisted;
            this.ntcpTransportUnreachable = ntcpTransportUnreachable;
            this.ntcp2TransportAvailable = ntcp2TransportAvailable;
            this.ntcp2BidAvailable = ntcp2BidAvailable;
            this.bidUsesNtcpTransport = bidUsesNtcpTransport;
            this.bidLatencyMs = bidLatencyMs;
            this.actualStoreMessageSize = actualStoreMessageSize;
            this.actualFloodSelectorIncludesCandidate = actualFloodSelectorIncludesCandidate;
            this.error = error;
        }
    }

    private P470TransportPreflight() {
    }

    public static Result inspect(RouterContext context, Hash candidateHash) {
        if (context == null || candidateHash == null)
            return unavailable("null-argument");
        try {
            boolean routerHidden = context.router().isHidden();
            boolean floodfillEnabled = context.netDb() instanceof FloodfillNetworkDatabaseFacade
                && ((FloodfillNetworkDatabaseFacade) context.netDb()).floodfillEnabled();
            RouterInfo local = context.router().getRouterInfo();
            RouterInfo candidate = context.netDb().lookupRouterInfoLocally(candidateHash);
            boolean acceptsRouterInfoStore = candidate != null && StoreJob.shouldStoreTo(candidate);
            if (local == null || candidate == null)
                return new Result(candidate != null, routerHidden, floodfillEnabled,
                                  acceptsRouterInfoStore, false, false, false, false,
                                  false, false, -1, -1, false, "router-info-unavailable");

            Hash routingKey = context.routingKeyGenerator()
                .getRoutingKey(local.getIdentity().calculateHash());
            P220SelectorProbe.Result selection = P220SelectorProbe.selectForKey(
                context, routingKey, candidateHash);
            if (selection.error != null)
                return new Result(true, routerHidden, floodfillEnabled,
                                  acceptsRouterInfoStore, false, false, false, false,
                                  false, false, -1, -1, false, "selector-unavailable");

            SortedMap<String, Transport> transports = context.commSystem().getTransports();
            Transport ntcp = transports.get(NTCPTransport.STYLE);
            boolean candidateBanlisted = context.banlist().isBanlisted(candidateHash);
            boolean ntcpUnreachable = ntcp != null && ntcp.isUnreachable(candidateHash);
            if (ntcp == null)
                return new Result(true, routerHidden, floodfillEnabled,
                                  acceptsRouterInfoStore, selection.selected.contains(candidateHash),
                                  candidateBanlisted, false, false, false, false, -1,
                                  -1, selection.selected.contains(candidateHash),
                                  "ntcp-transport-unavailable");

            // Match the real normal flood Store's encoded message size. This
            // diagnostic remains read-only: it does not submit the message.
            DatabaseStoreMessage store = new DatabaseStoreMessage(context);
            store.setEntry(local);
            int storeSize = store.getMessageSize();
            TransportBid bid = ntcp.bid(candidate, storeSize);
            if (bid == null)
                return new Result(true, routerHidden, floodfillEnabled, acceptsRouterInfoStore,
                                  selection.selected.contains(candidateHash), candidateBanlisted,
                                  ntcpUnreachable, true, false, false, -1, storeSize,
                                  selection.selected.contains(candidateHash), null);
            boolean usesNtcp = bid.getTransport() != null
                && NTCPTransport.STYLE.equals(bid.getTransport().getStyle());
            boolean available = bid.getLatencyMs() >= 0
                && bid.getLatencyMs() < TransportBid.TRANSIENT_FAIL;
            return new Result(true, routerHidden, floodfillEnabled, acceptsRouterInfoStore,
                              selection.selected.contains(candidateHash), candidateBanlisted,
                              ntcpUnreachable, true, available,
                              usesNtcp, bid.getLatencyMs(), storeSize,
                              selection.selected.contains(candidateHash), null);
        } catch (RuntimeException error) {
            return unavailable("stock-preflight-threw");
        }
    }

    private static Result unavailable(String reason) {
        return new Result(false, false, false, false, false, false, false, false,
                          false, false, -1, -1, false, reason);
    }

    /** Read-only lifetime event counters owned by the pinned stock router. */
    public static String counters(RouterContext context) {
        String[] names = {
            "transport.bidFailNoTransports", "transport.bidFailBanlisted",
            "transport.bidFailSelf", "transport.expiredOnQueueLifetime",
            "transport.sendMessageFailureLifetime", "ntcp.accept",
            "ntcp.registerConnect",
            "ntcp.connectSuccessful", "ntcp.inboundEstablished",
            "ntcp.outboundFailedIOEImmediate", "ntcp.outboundEstablishFailed",
            "ntcp.connectFailedIOE", "ntcp.connectFailedTimeout",
            "ntcp.connectFailedTimeoutIOE",
            "ntcp.zeroReadDrop", "ntcp.readError",
            "netDb.lookupsReceived", "netDb.lookupsDropped",
            "netDb.lookupsHandled", "netDb.lookupsMatched",
            "netDb.nonFFLookupsDropped", "netDb.DLMAllZeros"
        };
        StringBuilder result = new StringBuilder("P470-EV observable=true");
        for (String name : names) {
            RateStat stat = context.statManager().getRate(name);
            result.append(' ').append(name.replace('.', '_')).append('=')
                  .append(stat == null ? -1 : stat.getLifetimeEventCount());
        }
        return result.toString();
    }

    /**
     * Submit the local, NetDB-owned RouterInfo through the stock StoreJob
     * algorithm. This is needed when a floodfill's token-zero RI flood is
     * intentionally suppressed by a fresh NTCP2 recipient. The stock job
     * selects its floodfill target, creates the reply token and selector,
     * and enters CommSystem through its ordinary OutNetMessage path.
     */
    public static synchronized String submitReplyRequestedRouterInfoStore(RouterContext context) {
        if (context == null || !(context.netDb() instanceof FloodfillNetworkDatabaseFacade facade))
            return "P470-STORE unavailable=false reason=netdb-floodfill-unavailable";
        RouterInfo local = context.router().getRouterInfo();
        if (local == null || local.getType() != DatabaseEntry.KEY_TYPE_ROUTERINFO)
            return "P470-STORE unavailable=false reason=local-router-info-unavailable";
        if (STORE_SUBMITTED.get() != 0)
            return storeStatus();
        Job success = new JobImpl(context) {
            public String getName() { return "Plan 470 stock StoreJob success receipt"; }
            public void runJob() { STORE_SUCCEEDED.incrementAndGet(); }
        };
        Job failure = new JobImpl(context) {
            public String getName() { return "Plan 470 stock StoreJob failure receipt"; }
            public void runJob() { STORE_FAILED.incrementAndGet(); }
        };
        FloodfillStoreJob job = new FloodfillStoreJob(
            context, facade, local.getIdentity().calculateHash(), local,
            success, failure, 30_000L);
        STORE_SUBMITTED.incrementAndGet();
        context.jobQueue().addJob(job);
        return storeStatus();
    }

    public static String storeStatus() {
        return "P470-STORE submitted=" + STORE_SUBMITTED.get()
            + " succeeded=" + STORE_SUCCEEDED.get()
            + " failed=" + STORE_FAILED.get();
    }

    /** Read-only listener readiness; does not create an unframed TCP probe. */
    public static String listenerStatus(RouterContext context) {
        if (context == null)
            return "P470-LISTENER listening=false";
        SortedMap<String, Transport> transports = context.commSystem().getTransports();
        Transport transport = transports.get(NTCPTransport.STYLE);
        boolean listening = transport instanceof NTCPTransport
            && ((NTCPTransport) transport).isAlive()
            && transport.hasCurrentAddress();
        return "P470-LISTENER listening=" + listening;
    }

    /**
     * Read the pinned stock LookupThrottler's admission limit without calling
     * shouldThrottleLookup(), which increments its per-peer counter. Java
     * 2.13.0 sets this limit on the first two-minute Cleaner tick, so it is
     * zero during early startup and the first valid lookup is dropped.
     */
    public static String lookupThrottleStatus(RouterContext context) {
        if (context == null || !(context.netDb() instanceof FloodfillNetworkDatabaseFacade))
            return "P470-LOOKUP ready=false";
        try {
            Field ownerField = FloodfillNetworkDatabaseFacade.class
                .getDeclaredField("_lookupThrottler");
            ownerField.setAccessible(true);
            Object throttler = ownerField.get(context.netDb());
            if (throttler == null)
                return "P470-LOOKUP ready=false";
            Field limitField = throttler.getClass().getDeclaredField("_max");
            limitField.setAccessible(true);
            int limit = limitField.getInt(throttler);
            return "P470-LOOKUP ready=" + (limit > 0);
        } catch (ReflectiveOperationException | SecurityException error) {
            return "P470-LOOKUP ready=false";
        }
    }
}
