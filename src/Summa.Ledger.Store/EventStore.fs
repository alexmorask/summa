namespace Summa.Ledger.Store

open System
open System.Threading.Tasks
open Summa.Ledger.Domain

type AppendResult =
    | Appended of seq: int64
    | Duplicate of existingId: Guid

type StoredEvent =
    { Seq   : int64
      Event : TransactionPosted }

type EventStore =
    { Append   : TransactionPosted -> Task<AppendResult>
      ReadFrom : int64 -> Task<StoredEvent list> }
