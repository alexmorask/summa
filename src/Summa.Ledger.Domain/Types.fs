namespace Summa.Ledger.Domain

open System

type AccountId = AccountId of string

type Money = int64

type Direction =
    | Debit
    | Credit

type Entry =
    { Account   : AccountId
      Direction : Direction
      Amount    : Money }

type Transaction =
    { Id             : Guid
      OccurredAt     : DateTimeOffset
      Description    : string
      CorrelationId  : string
      CausationId    : string
      IdempotencyKey : string
      Entries        : Entry list }

type LedgerError =
    | TooFewEntries
    | NonPositiveAmount
    | Unbalanced of debits: int64 * credits: int64
