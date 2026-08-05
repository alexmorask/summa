module ObligationEventStoreTests

open System
open System.Threading.Tasks
open Xunit
open Npgsql
open Npgsql.FSharp
open Summa.Recognition.Domain
open Summa.Recognition.Store

let private dataSource =
    NpgsqlDataSource.Create "Host=localhost;Port=5432;Username=summa;Password=summa;Database=summa"

let private store = PostgresObligationEventStore.create dataSource

let private newObligation idempotencyKey =
    let id = PerformanceObligationId (Guid.NewGuid())
    let startDate = DateTimeOffset(2026, 1, 1, 0, 0, 0, TimeSpan.Zero)
    match PerformanceObligation.create id 12000L 12 startDate "correlation-1" "causation-1" idempotencyKey with
    | Ok obligation -> ObligationCreated obligation
    | Error e -> failwith $"expected Ok, got Error {e}"

[<Fact>]
let ``appending a new obligation returns Appended with a sequence number`` () : Task =
    task {
        let! result = store.Append(newObligation (Guid.NewGuid().ToString()))
        match result with
        | Appended seq -> Assert.True(seq > 0L)
        | Duplicate _ -> failwith "expected Appended, got Duplicate"
    }

[<Fact>]
let ``appending the same idempotency key twice returns Duplicate and leaves exactly one row`` () : Task =
    task {
        let idempotencyKey = Guid.NewGuid().ToString()
        let (ObligationCreated obligation) as posted = newObligation idempotencyKey
        let (PerformanceObligationId rawId) = obligation.Id

        let! first = store.Append posted
        let! second = store.Append posted

        match first, second with
        | Appended _, Duplicate existingId -> Assert.Equal(rawId, existingId)
        | _ -> failwith $"expected Appended then Duplicate, got {first}, {second}"

        let! count =
            dataSource
            |> Sql.fromDataSource
            |> Sql.query "SELECT count(*) AS count FROM recognition.obligation_events WHERE idempotency_key = @key;"
            |> Sql.parameters [ "key", Sql.string idempotencyKey ]
            |> Sql.executeRowAsync (fun read -> read.int64 "count")
        Assert.Equal(1L, count)
    }

[<Fact>]
let ``ReadFrom returns appended events in order with matching content`` () : Task =
    task {
        let (ObligationCreated obligation) as posted = newObligation (Guid.NewGuid().ToString())
        let! appendResult = store.Append posted
        let seq = match appendResult with Appended s -> s | Duplicate _ -> failwith "expected Appended"

        let! events = store.ReadFrom (seq - 1L)
        let stored = events |> List.find (fun e -> e.Seq = seq)
        let (ObligationCreated roundTripped) = stored.Event
        Assert.Equal(obligation.Id, roundTripped.Id)
        Assert.Equal(obligation.TotalAmount, roundTripped.TotalAmount)
        Assert.Equal(obligation.PeriodCount, roundTripped.PeriodCount)
    }
