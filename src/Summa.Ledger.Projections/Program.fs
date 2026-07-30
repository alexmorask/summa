module Summa.Ledger.Projections.Program

open System
open Microsoft.Extensions.Configuration
open Microsoft.Extensions.Hosting
open Npgsql
open Summa.Ledger.Store

[<EntryPoint>]
let main args =
    let builder = Host.CreateApplicationBuilder(args)
    let connectionString = builder.Configuration.GetConnectionString("Summa")
    let dataSource = NpgsqlDataSource.Create(connectionString)
    let eventStore = PostgresEventStore.create dataSource

    Worker.run dataSource eventStore (TimeSpan.FromMilliseconds 500.0)
    |> fun task -> task.GetAwaiter().GetResult()

    0
