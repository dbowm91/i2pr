// Plan 470 read-only Java selector and NTCP transport bid preflight.
// Compiled out-of-tree against the exact pinned stock jars. It calls the
// stock floodfill selector and transport bid APIs; it does not send I2NP.
package net.i2p.router.networkdb.kademlia;

import java.util.SortedMap;
import net.i2p.data.Hash;
import net.i2p.data.router.RouterInfo;
import net.i2p.router.RouterContext;
import net.i2p.router.transport.Transport;
import net.i2p.router.transport.TransportBid;
import net.i2p.router.transport.ntcp.NTCPTransport;

public final class P470TransportPreflight {

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
        public final String error;

        private Result(boolean candidatePresent, boolean routerHidden,
                       boolean floodfillEnabled,
                       boolean acceptsRouterInfoStore,
                       boolean selectedForLocalRouterInfo,
                       boolean candidateBanlisted, boolean ntcpTransportUnreachable,
                       boolean ntcp2TransportAvailable, boolean ntcp2BidAvailable,
                       boolean bidUsesNtcpTransport, int bidLatencyMs, String error) {
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
                                  acceptsRouterInfoStore, false, false, false, false, false, false, -1,
                                  "router-info-unavailable");

            Hash routingKey = context.routingKeyGenerator()
                .getRoutingKey(local.getIdentity().calculateHash());
            P220SelectorProbe.Result selection = P220SelectorProbe.selectForKey(
                context, routingKey, candidateHash);
            if (selection.error != null)
                return new Result(true, routerHidden, floodfillEnabled,
                                  acceptsRouterInfoStore, false, false, false, false, false, false, -1,
                                  "selector-unavailable");

            SortedMap<String, Transport> transports = context.commSystem().getTransports();
            Transport ntcp = transports.get(NTCPTransport.STYLE);
            boolean candidateBanlisted = context.banlist().isBanlisted(candidateHash);
            boolean ntcpUnreachable = ntcp != null && ntcp.isUnreachable(candidateHash);
            if (ntcp == null)
                return new Result(true, routerHidden, floodfillEnabled,
                                  acceptsRouterInfoStore, selection.selected.contains(candidateHash),
                                  candidateBanlisted, false, false, false, false, -1,
                                  "ntcp-transport-unavailable");

            // The pinned normal flood path submits a RouterInfo Store. This
            // bounded size exercises the same NTCP bid eligibility without
            // submitting a message or opening a socket.
            TransportBid bid = ntcp.bid(candidate, 1024);
            if (bid == null)
                return new Result(true, routerHidden, floodfillEnabled, acceptsRouterInfoStore,
                                  selection.selected.contains(candidateHash), candidateBanlisted,
                                  ntcpUnreachable, true, false, false, -1, null);
            boolean usesNtcp = bid.getTransport() != null
                && NTCPTransport.STYLE.equals(bid.getTransport().getStyle());
            boolean available = bid.getLatencyMs() >= 0
                && bid.getLatencyMs() < TransportBid.TRANSIENT_FAIL;
            return new Result(true, routerHidden, floodfillEnabled, acceptsRouterInfoStore,
                              selection.selected.contains(candidateHash), candidateBanlisted,
                              ntcpUnreachable, true, available,
                              usesNtcp, bid.getLatencyMs(), null);
        } catch (RuntimeException error) {
            return unavailable("stock-preflight-threw");
        }
    }

    private static Result unavailable(String reason) {
        return new Result(false, false, false, false, false, false, false, false,
                          false, false, -1, reason);
    }
}
