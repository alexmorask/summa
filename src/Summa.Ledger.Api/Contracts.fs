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

type AccountBalanceResponse = { AccountId : string; Balance : int64 }

type TrialBalanceResponse = { Balance : int64; Balanced : bool }

type ErrorResponse = { Error : string }
