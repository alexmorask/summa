module RunnerTests

open System
open System.Threading.Tasks
open Xunit
open Npgsql
open Npgsql.FSharp
open Summa.Recognition.Domain
open Summa.Recognition.Store
open Summa.Ledger.Domain
open Summa.Ledger.Store
open Summa.Recognition.Job

let private dataSource =
    NpgsqlDataSource.Create "Host=localhost;Port=5432;Username=summa;Password=summa;Database=summa"

let private obligationStore = PostgresObligationEventStore.create dataSource
let private ledgerStore = PostgresEventStore.create dataSource

let private startDate = DateTimeOffset(2026, 1, 1, 0, 0, 0, TimeSpan.Zero)

/// Creates and appends a fresh $120/12-month obligation (the ledger.md worked example),
/// returning its correlation id so tests can query the transactions it produces.
let private newObligation () : Task<string> =
    task {
        let id = PerformanceObligationId (Guid.NewGuid())
        let correlationId = $"purchase:{Guid.NewGuid()}"
        let obligation =
            PerformanceObligation.create id 12000L 12 startDate correlationId "AgreementActivated:test" (Guid.NewGuid().ToString())
            |> Result.defaultWith (fun e -> failwith $"expected Ok, got Error {e}")
        let! _ = obligationStore.Append(ObligationCreated obligation)
        return correlationId
    }

let private transactionsFor (correlationId: string) : Task<int64 list> =
    dataSource
    |> Sql.fromDataSource
    |> Sql.query "SELECT payload FROM ledger.events WHERE correlation_id = @correlation_id ORDER BY seq;"
    |> Sql.parameters [ "correlation_id", Sql.string correlationId ]
    |> Sql.executeAsync (fun read ->
        let transaction = Summa.Ledger.Store.Serialization.deserialize (read.text "payload")
        transaction.Entries |> List.find (fun e -> e.Direction = Credit) |> fun e -> e.Amount)

[<Fact>]
let ``running the job once posts only the periods due by then`` () : Task =
    task {
        let! correlationId = newObligation ()

        do! Runner.run obligationStore ledgerStore startDate

        let! amounts = transactionsFor correlationId
        Assert.Equal<int64 list>([ 1000L ], amounts)
    }

[<Fact>]
let ``running the job with a later date posts every period up to and including it`` () : Task =
    task {
        let! correlationId = newObligation ()

        do! Runner.run obligationStore ledgerStore (startDate.AddMonths 11)

        let! amounts = transactionsFor correlationId
        Assert.Equal(12, List.length amounts)
        Assert.Equal(12000L, List.sum amounts)
    }

[<Fact>]
let ``running the job twice in a row posts nothing the second time`` () : Task =
    task {
        let! correlationId = newObligation ()
        let asOf = startDate.AddMonths 11

        do! Runner.run obligationStore ledgerStore asOf
        do! Runner.run obligationStore ledgerStore asOf

        let! amounts = transactionsFor correlationId
        Assert.Equal(12, List.length amounts)
    }

[<Fact>]
let ``running the job after skipping periods catches up without duplicating`` () : Task =
    task {
        let! correlationId = newObligation ()

        do! Runner.run obligationStore ledgerStore (startDate.AddMonths 2)
        let! afterFirstRun = transactionsFor correlationId
        Assert.Equal(3, List.length afterFirstRun)

        do! Runner.run obligationStore ledgerStore (startDate.AddMonths 11)
        let! afterCatchUp = transactionsFor correlationId
        Assert.Equal(12, List.length afterCatchUp)
        Assert.Equal(12000L, List.sum afterCatchUp)
    }
