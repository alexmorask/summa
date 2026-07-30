namespace Summa.Ledger.Store

open System
open System.Threading.Tasks
open Summa.Ledger.Domain

module Handler =

    let handle (store: EventStore) (command: PostTransaction) : Task<Result<Guid, LedgerError>> =
        task {
            match Ledger.decide () command with
            | Error error -> return Error error
            | Ok events ->
                let TransactionPosted transaction as posted = List.exactlyOne events
                let! result = store.Append posted
                match result with
                | Appended _ -> return Ok transaction.Id
                | Duplicate existingId -> return Ok existingId
        }
