module HandlerTests

open System
open System.Threading.Tasks
open Xunit
open Summa.Ledger.Domain
open Summa.Ledger.Store

let private entry account direction amount =
    { Account = AccountId account; Direction = direction; Amount = amount }

let private command idempotencyKey entries : PostTransaction =
    { Id = Guid.NewGuid()
      OccurredAt = DateTimeOffset.UtcNow
      Description = "test command"
      CorrelationId = "correlation-1"
      CausationId = "causation-1"
      IdempotencyKey = idempotencyKey
      Entries = entries }

let private inMemoryStore () : EventStore =
    let events = ResizeArray<StoredEvent>()

    let append (TransactionPosted transaction as posted) =
        task {
            let existingId =
                events
                |> Seq.tryPick (fun stored ->
                    let (TransactionPosted existing) = stored.Event
                    if existing.IdempotencyKey = transaction.IdempotencyKey then Some existing.Id else None)
            match existingId with
            | Some existingId -> return Duplicate existingId
            | None ->
                let seq = int64 events.Count + 1L
                events.Add { Seq = seq; Event = posted }
                return Appended seq
        }

    let readFrom (lastSeq: int64) =
        task { return events |> Seq.filter (fun e -> e.Seq > lastSeq) |> List.ofSeq }

    { Append = append; ReadFrom = readFrom }

let private balancedEntries =
    [ entry "ar:cust_123" Debit 12000L
      entry "deferred_revenue" Credit 12000L ]

[<Fact>]
let ``valid command appends and returns the new transaction id`` () : Task =
    task {
        let store = inMemoryStore ()
        let cmd = command (Guid.NewGuid().ToString()) balancedEntries

        let! result = Handler.handle store cmd

        Assert.Equal(Ok cmd.Id, result)
    }

[<Fact>]
let ``duplicate idempotency key returns the existing id as a success`` () : Task =
    task {
        let store = inMemoryStore ()
        let idempotencyKey = Guid.NewGuid().ToString()
        let first = command idempotencyKey balancedEntries
        let second = command idempotencyKey balancedEntries

        let! firstResult = Handler.handle store first
        let! secondResult = Handler.handle store second

        Assert.Equal(Ok first.Id, firstResult)
        Assert.Equal(Ok first.Id, secondResult)
    }

[<Fact>]
let ``invalid command returns the LedgerError and appends nothing`` () : Task =
    task {
        let store = inMemoryStore ()
        let unbalanced =
            [ entry "ar:cust_123" Debit 12000L
              entry "deferred_revenue" Credit 11000L ]
        let cmd = command (Guid.NewGuid().ToString()) unbalanced

        let! result = Handler.handle store cmd

        Assert.Equal(Error (Unbalanced (12000L, 11000L)), result)
        let! stored = store.ReadFrom 0L
        Assert.Empty(stored)
    }
