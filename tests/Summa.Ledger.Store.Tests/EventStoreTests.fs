module EventStoreTests

open System
open System.Threading.Tasks
open Xunit
open Npgsql
open Npgsql.FSharp
open Summa.Ledger.Domain
open Summa.Ledger.Store

let private dataSource =
    NpgsqlDataSource.Create "Host=localhost;Port=5432;Username=summa;Password=summa;Database=summa"

let private store = PostgresEventStore.create dataSource

let private entry account direction amount =
    { Account = AccountId account; Direction = direction; Amount = amount }

let private postedTransaction idempotencyKey =
    let entries = [ entry "ar:cust_123" Debit 12000L; entry "deferred_revenue" Credit 12000L ]
    match Transaction.create (Guid.NewGuid()) DateTimeOffset.UtcNow "test transaction"
              (Guid.NewGuid().ToString()) (Guid.NewGuid().ToString()) idempotencyKey entries with
    | Ok txn -> TransactionPosted txn
    | Error e -> failwith $"expected Ok, got Error {e}"

[<Fact>]
let ``appending a new transaction returns Appended with a sequence number`` () : Task =
    task {
        let! result = store.Append(postedTransaction (Guid.NewGuid().ToString()))
        match result with
        | Appended seq -> Assert.True(seq > 0L)
        | Duplicate _ -> failwith "expected Appended, got Duplicate"
    }

[<Fact>]
let ``appending the same idempotency key twice returns Duplicate and leaves exactly one row`` () : Task =
    task {
        let idempotencyKey = Guid.NewGuid().ToString()
        let TransactionPosted transaction as posted = postedTransaction idempotencyKey

        let! first = store.Append posted
        let! second = store.Append posted

        match first, second with
        | Appended _, Duplicate existingId -> Assert.Equal(transaction.Id, existingId)
        | _ -> failwith $"expected Appended then Duplicate, got {first}, {second}"

        let! count =
            dataSource
            |> Sql.fromDataSource
            |> Sql.query "SELECT count(*) AS count FROM ledger.events WHERE idempotency_key = @key;"
            |> Sql.parameters [ "key", Sql.string idempotencyKey ]
            |> Sql.executeRowAsync (fun read -> read.int64 "count")
        Assert.Equal(1L, count)
    }

[<Fact>]
let ``ReadFrom returns appended events in order with matching content`` () : Task =
    task {
        let TransactionPosted transaction as posted = postedTransaction (Guid.NewGuid().ToString())
        let! appendResult = store.Append posted
        let seq = match appendResult with Appended s -> s | Duplicate _ -> failwith "expected Appended"

        let! events = store.ReadFrom (seq - 1L)
        let stored = events |> List.find (fun e -> e.Seq = seq)
        let (TransactionPosted roundTripped) = stored.Event
        Assert.Equal(transaction.Id, roundTripped.Id)
        Assert.Equal<Entry list>(transaction.Entries, roundTripped.Entries)
    }
