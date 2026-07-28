namespace Summa.Ledger.Domain

open System

type PostTransaction =
    { Id             : Guid
      OccurredAt     : DateTimeOffset
      Description    : string
      CorrelationId  : string
      CausationId    : string
      IdempotencyKey : string
      Entries        : Entry list }

type TransactionPosted = TransactionPosted of Transaction

module Ledger =

    let decide (_state: unit) (command: PostTransaction) : Result<TransactionPosted list, LedgerError> =
        Transaction.create
            command.Id command.OccurredAt command.Description
            command.CorrelationId command.CausationId command.IdempotencyKey
            command.Entries
        |> Result.map (fun transaction -> [ TransactionPosted transaction ])
