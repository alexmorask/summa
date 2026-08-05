namespace Summa.Recognition.Store

open System
open System.Threading.Tasks
open Summa.Recognition.Domain

type AppendResult =
    | Appended of seq: int64
    | Duplicate of existingId: Guid

type StoredObligationEvent =
    { Seq   : int64
      Event : ObligationCreated }

type ObligationEventStore =
    { Append   : ObligationCreated -> Task<AppendResult>
      ReadFrom : int64 -> Task<StoredObligationEvent list> }
