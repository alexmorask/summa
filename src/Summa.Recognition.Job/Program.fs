module Summa.Recognition.Job.Program

open System
open Npgsql
open Summa.Recognition.Store
open Summa.Ledger.Store

[<EntryPoint>]
let main _ =
    let connectionString = Environment.GetEnvironmentVariable "ConnectionStrings__Summa"
    let dataSource = NpgsqlDataSource.Create connectionString
    let obligationStore = PostgresObligationEventStore.create dataSource
    let ledgerStore = PostgresEventStore.create dataSource

    Runner.run obligationStore ledgerStore DateTimeOffset.UtcNow
    |> fun task -> task.GetAwaiter().GetResult()

    0
