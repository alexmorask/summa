module Summa.Ledger.Projections.Program

open System
open Microsoft.Extensions.Configuration
open Microsoft.Extensions.DependencyInjection
open Microsoft.Extensions.Hosting
open Npgsql
open Summa.Ledger.Store

[<EntryPoint>]
let main args =
    let builder = Host.CreateApplicationBuilder(args)
    let connectionString = builder.Configuration.GetConnectionString("Summa")
    let dataSource = NpgsqlDataSource.Create(connectionString)
    let eventStore = PostgresEventStore.create dataSource

    builder.Services.AddHostedService<Worker.HostedService>(fun _ ->
        new Worker.HostedService(dataSource, eventStore, TimeSpan.FromMilliseconds 500.0))
    |> ignore

    builder.Build().Run()

    0
