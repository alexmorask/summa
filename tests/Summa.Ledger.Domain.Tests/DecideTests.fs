module DecideTests

open System
open Xunit
open Summa.Ledger.Domain

let private entry account direction amount =
    { Account = AccountId account; Direction = direction; Amount = amount }

let private command entries : PostTransaction =
    { Id = Guid.NewGuid()
      OccurredAt = DateTimeOffset.UtcNow
      Description = "test command"
      CorrelationId = "correlation-1"
      CausationId = "causation-1"
      IdempotencyKey = "idempotency-1"
      Entries = entries }

[<Fact>]
let ``valid command yields exactly one TransactionPosted event`` () =
    let entries =
        [ entry "ar:cust_123" Debit 12000L
          entry "deferred_revenue" Credit 12000L ]
    match Ledger.decide () (command entries) with
    | Ok [ TransactionPosted transaction ] ->
        Assert.Equal<Entry list>(entries, transaction.Entries)
    | other -> failwith $"expected Ok with exactly one event, got {other}"

[<Fact>]
let ``invalid command yields the same error Transaction.create would produce`` () =
    let entries =
        [ entry "ar:cust_123" Debit 12000L
          entry "deferred_revenue" Credit 11000L ]
    Assert.Equal(Error (Unbalanced (12000L, 11000L)), Ledger.decide () (command entries))
