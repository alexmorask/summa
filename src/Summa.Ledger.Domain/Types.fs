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

module Transaction =

    let create id occurredAt description correlation causation idem (entries: Entry list) =
        let total direction =
            entries
            |> List.sumBy (fun entry -> if entry.Direction = direction then entry.Amount else 0L)
        let debits, credits = total Debit, total Credit
        if List.length entries < 2 then
            Error TooFewEntries
        elif entries |> List.exists (fun entry -> entry.Amount <= 0L) then
            Error NonPositiveAmount
        elif debits <> credits then
            Error (Unbalanced (debits, credits))
        else
            Ok { Id = id
                 OccurredAt = occurredAt
                 Description = description
                 CorrelationId = correlation
                 CausationId = causation
                 IdempotencyKey = idem
                 Entries = entries }
