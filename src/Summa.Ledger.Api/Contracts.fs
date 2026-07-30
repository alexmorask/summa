namespace Summa.Ledger.Api

open System
open Summa.Ledger.Domain

type PostTransactionRequest =
    { OccurredAt     : DateTimeOffset
      Description    : string
      CorrelationId  : string
      CausationId    : string
      IdempotencyKey : string
      Entries        : Entry list }

type PostTransactionResponse = { Id : Guid }

type ErrorResponse = { Error : string }
