namespace Summa.Recognition.Store

open System
open System.Threading.Tasks
open Summa.Recognition.Domain

type AppendResult =
    | Appended of seq: int64
    | Duplicate of existingId: Guid

type StoredEvent =
    { Seq   : int64
      Event : ObligationCreated }

type EventStore =
    { Append   : ObligationCreated -> Task<AppendResult>
      ReadFrom : int64 -> Task<StoredEvent list> }
