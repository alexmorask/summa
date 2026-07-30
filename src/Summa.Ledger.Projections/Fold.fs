namespace Summa.Ledger.Projections

open Summa.Ledger.Domain
open Summa.Ledger.Store

module Fold =

    let deltas (events: StoredEvent list) : Map<AccountId, int64> =
        events
        |> List.collect (fun stored ->
            let (TransactionPosted transaction) = stored.Event
            transaction.Entries)
        |> List.fold
            (fun balances entry ->
                let signedAmount =
                    match entry.Direction with
                    | Debit -> entry.Amount
                    | Credit -> -entry.Amount
                balances |> Map.change entry.Account (fun existing -> Some (defaultArg existing 0L + signedAmount)))
            Map.empty
