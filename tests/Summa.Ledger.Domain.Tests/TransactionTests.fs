module TransactionTests

open System
open Xunit
open Summa.Ledger.Domain

let private entry account direction amount =
    { Account = AccountId account; Direction = direction; Amount = amount }

let private create entries =
    Transaction.create (Guid.NewGuid()) DateTimeOffset.UtcNow "test transaction"
        "correlation-1" "causation-1" "idempotency-1" entries

[<Fact>]
let ``balanced two-entry transaction succeeds`` () =
    let entries =
        [ entry "ar:cust_123" Debit 12000L
          entry "deferred_revenue" Credit 12000L ]
    match create entries with
    | Ok txn -> Assert.Equal<Entry list>(entries, txn.Entries)
    | Error e -> failwith $"expected Ok, got Error {e}"

[<Fact>]
let ``balanced three-entry transaction succeeds (service + tax)`` () =
    let entries =
        [ entry "ar:cust_123" Debit 13200L
          entry "deferred_revenue" Credit 12000L
          entry "tax_payable:CA" Credit 1200L ]
    match create entries with
    | Ok txn -> Assert.Equal<Entry list>(entries, txn.Entries)
    | Error e -> failwith $"expected Ok, got Error {e}"

[<Fact>]
let ``unbalanced entries return Unbalanced`` () =
    let entries =
        [ entry "ar:cust_123" Debit 12000L
          entry "deferred_revenue" Credit 11000L ]
    Assert.Equal(Error (Unbalanced (12000L, 11000L)), create entries)

[<Fact>]
let ``fewer than two entries returns TooFewEntries`` () =
    let entries = [ entry "cash" Debit 100L ]
    Assert.Equal(Error TooFewEntries, create entries)

[<Fact>]
let ``non-positive amount returns NonPositiveAmount`` () =
    let entries =
        [ entry "cash" Debit 0L
          entry "ar:cust_123" Credit 0L ]
    Assert.Equal(Error NonPositiveAmount, create entries)
