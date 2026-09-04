module Summa.Recognition.Job.Program

open System
open Npgsql
open Summa.Recognition.Store
open Summa.Ledger.Store

[<EntryPoint>]
let main _ =
    let connectionString = Environment.GetEnvironmentVariable "ConnectionStrings__Summa"
    let dataSource = NpgsqlDataSource.Create connectionString
    let obligationStore = Summa.Recognition.Store.PostgresEventStore.create dataSource
    let ledgerStore = Summa.Ledger.Store.PostgresEventStore.create dataSource

    Runner.run obligationStore ledgerStore DateTimeOffset.UtcNow
    |> fun task -> task.GetAwaiter().GetResult()

    0
