// ALICE-Settlement — Unity C# Bindings
// License: AGPL-3.0-only
// Author: Moroya Sakamoto
//
// 22 DllImport + IDisposable RAII wrappers

using System;
using System.Runtime.InteropServices;

namespace Alice.Settlement
{
    // ── FFI structs ────────────────────────────────────────────────

    [StructLayout(LayoutKind.Sequential)]
    public struct FfiNetObligation
    {
        public ulong symbolHash;
        public ulong delivererId;
        public ulong receiverId;
        public ulong netQuantity;
        public long  netPayment;
        public uint  tradeCount;
        public uint  _pad;
    }

    [StructLayout(LayoutKind.Sequential)]
    public struct FfiMarginRequirement
    {
        public ulong accountId;
        public long  initialMargin;
        public long  variationMargin;
        public long  stressMargin;
        public long  totalMargin;
        public ulong contentHash;
    }

    [StructLayout(LayoutKind.Sequential)]
    public struct FfiWaterfallResult
    {
        public long  totalLoss;
        public long  totalAbsorbed;
        public byte  fullyCovered;
        public long  shortfall;
        public ulong contentHash;
    }

    // ── P/Invoke declarations ──────────────────────────────────────

    internal static class Native
    {
#if UNITY_IOS && !UNITY_EDITOR
        private const string Lib = "__Internal";
#else
        private const string Lib = "alice_settlement";
#endif

        // NettingEngine
        [DllImport(Lib)] public static extern IntPtr alice_netting_engine_new();
        [DllImport(Lib)] public static extern void alice_netting_engine_add_trade(
            IntPtr engine, ulong tradeId, ulong symbolHash,
            ulong buyerId, ulong sellerId, long price,
            ulong quantity, ulong timestampNs, byte status);
        [DllImport(Lib)] public static extern IntPtr alice_netting_engine_compute_net(
            IntPtr engine, out uint outLen);
        [DllImport(Lib)] public static extern void alice_obligations_free(IntPtr ptr, uint len);
        [DllImport(Lib)] public static extern void alice_netting_engine_clear(IntPtr engine);
        [DllImport(Lib)] public static extern void alice_netting_engine_destroy(IntPtr engine);

        // ClearingHouse
        [DllImport(Lib)] public static extern IntPtr alice_clearing_house_new();
        [DllImport(Lib)] public static extern void alice_clearing_house_register_account(
            IntPtr ch, ulong id, long initialBalance);
        [DllImport(Lib)] public static extern long alice_clearing_house_get_balance(
            IntPtr ch, ulong id);
        [DllImport(Lib)] public static extern int alice_clearing_house_clear_obligation(
            IntPtr ch, ulong symbolHash, ulong delivererId, ulong receiverId,
            ulong netQuantity, long netPayment, uint tradeCount);
        [DllImport(Lib)] public static extern void alice_clearing_house_destroy(IntPtr ch);

        // MarginEngine
        [DllImport(Lib)] public static extern IntPtr alice_margin_engine_new(
            double initialMarginRate, double variationMarginRate, long marginFloor);
        [DllImport(Lib)] public static extern int alice_margin_engine_compute_obligation(
            IntPtr engine, ulong delivererId, ulong receiverId,
            ulong netQuantity, long netPayment, out FfiMarginRequirement result);
        [DllImport(Lib)] public static extern int alice_margin_engine_compute_portfolio(
            IntPtr engine, ulong accountId,
            [In] FfiNetObligation[] obligations, uint len,
            out FfiMarginRequirement result);
        [DllImport(Lib)] public static extern void alice_margin_engine_destroy(IntPtr engine);

        // DefaultWaterfall
        [DllImport(Lib)] public static extern IntPtr alice_waterfall_new(
            long defaulterMargin, long defaulterFund, long ccpFirstLoss,
            long membersFund, long ccpCapital);
        [DllImport(Lib)] public static extern int alice_waterfall_absorb_loss(
            IntPtr wf, long loss, out FfiWaterfallResult result);
        [DllImport(Lib)] public static extern long alice_waterfall_total_capacity(IntPtr wf);
        [DllImport(Lib)] public static extern void alice_waterfall_destroy(IntPtr wf);

        // SettlementJournal
        [DllImport(Lib)] public static extern IntPtr alice_journal_new();
        [DllImport(Lib)] public static extern void alice_journal_record_trade(
            IntPtr journal, ulong timestampNs, ulong tradeId);
        [DllImport(Lib)] public static extern uint alice_journal_len(IntPtr journal);
        [DllImport(Lib)] public static extern void alice_journal_destroy(IntPtr journal);
    }

    // ── RAII wrappers ──────────────────────────────────────────────

    public sealed class NettingEngine : IDisposable
    {
        private IntPtr _ptr;
        public NettingEngine() => _ptr = Native.alice_netting_engine_new();
        public void AddTrade(ulong tradeId, ulong symbolHash,
            ulong buyerId, ulong sellerId, long price,
            ulong quantity, ulong timestampNs, byte status)
            => Native.alice_netting_engine_add_trade(
                _ptr, tradeId, symbolHash, buyerId, sellerId,
                price, quantity, timestampNs, status);
        public FfiNetObligation[] ComputeNet()
        {
            var ptr = Native.alice_netting_engine_compute_net(_ptr, out uint len);
            if (ptr == IntPtr.Zero || len == 0) return Array.Empty<FfiNetObligation>();
            var result = new FfiNetObligation[len];
            int size = Marshal.SizeOf<FfiNetObligation>();
            for (int i = 0; i < len; i++)
                result[i] = Marshal.PtrToStructure<FfiNetObligation>(ptr + i * size);
            Native.alice_obligations_free(ptr, len);
            return result;
        }
        public void Clear() => Native.alice_netting_engine_clear(_ptr);
        public void Dispose() { if (_ptr != IntPtr.Zero) { Native.alice_netting_engine_destroy(_ptr); _ptr = IntPtr.Zero; } }
        ~NettingEngine() => Dispose();
    }

    public sealed class ClearingHouse : IDisposable
    {
        private IntPtr _ptr;
        public ClearingHouse() => _ptr = Native.alice_clearing_house_new();
        public void RegisterAccount(ulong id, long initialBalance)
            => Native.alice_clearing_house_register_account(_ptr, id, initialBalance);
        public long GetBalance(ulong id) => Native.alice_clearing_house_get_balance(_ptr, id);
        public int ClearObligation(ulong symbolHash, ulong delivererId, ulong receiverId,
            ulong netQuantity, long netPayment, uint tradeCount)
            => Native.alice_clearing_house_clear_obligation(
                _ptr, symbolHash, delivererId, receiverId, netQuantity, netPayment, tradeCount);
        public void Dispose() { if (_ptr != IntPtr.Zero) { Native.alice_clearing_house_destroy(_ptr); _ptr = IntPtr.Zero; } }
        ~ClearingHouse() => Dispose();
    }

    public sealed class MarginEngine : IDisposable
    {
        private IntPtr _ptr;
        public MarginEngine(double initialRate, double variationRate, long floor)
            => _ptr = Native.alice_margin_engine_new(initialRate, variationRate, floor);
        public FfiMarginRequirement ComputeObligation(
            ulong delivererId, ulong receiverId, ulong netQuantity, long netPayment)
        {
            Native.alice_margin_engine_compute_obligation(
                _ptr, delivererId, receiverId, netQuantity, netPayment, out var req);
            return req;
        }
        public FfiMarginRequirement ComputePortfolio(ulong accountId, FfiNetObligation[] obs)
        {
            Native.alice_margin_engine_compute_portfolio(
                _ptr, accountId, obs, (uint)obs.Length, out var req);
            return req;
        }
        public void Dispose() { if (_ptr != IntPtr.Zero) { Native.alice_margin_engine_destroy(_ptr); _ptr = IntPtr.Zero; } }
        ~MarginEngine() => Dispose();
    }

    public sealed class DefaultWaterfall : IDisposable
    {
        private IntPtr _ptr;
        public DefaultWaterfall(long dm, long df, long cfl, long mf, long cc)
            => _ptr = Native.alice_waterfall_new(dm, df, cfl, mf, cc);
        public FfiWaterfallResult AbsorbLoss(long loss)
        {
            Native.alice_waterfall_absorb_loss(_ptr, loss, out var result);
            return result;
        }
        public long TotalCapacity => Native.alice_waterfall_total_capacity(_ptr);
        public void Dispose() { if (_ptr != IntPtr.Zero) { Native.alice_waterfall_destroy(_ptr); _ptr = IntPtr.Zero; } }
        ~DefaultWaterfall() => Dispose();
    }

    public sealed class SettlementJournal : IDisposable
    {
        private IntPtr _ptr;
        public SettlementJournal() => _ptr = Native.alice_journal_new();
        public void RecordTrade(ulong timestampNs, ulong tradeId)
            => Native.alice_journal_record_trade(_ptr, timestampNs, tradeId);
        public uint Length => Native.alice_journal_len(_ptr);
        public void Dispose() { if (_ptr != IntPtr.Zero) { Native.alice_journal_destroy(_ptr); _ptr = IntPtr.Zero; } }
        ~SettlementJournal() => Dispose();
    }
}
