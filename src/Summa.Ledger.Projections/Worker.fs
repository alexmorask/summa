namespace Summa.Ledger.Projections

open System
open System.Threading
open System.Threading.Tasks
open Microsoft.Extensions.Hosting
open Npgsql
open Npgsql.FSharp
open Summa.Ledger.Domain
open Summa.Ledger.Store

module Worker =

    let private readCheckpoint (dataSource: NpgsqlDataSource) : Task<int64> =
        dataSource
        |> Sql.fromDataSource
        |> Sql.query "SELECT last_seq FROM ledger.account_balances_checkpoint WHERE id = 1;"
        |> Sql.executeRowAsync (fun read -> read.int64 "last_seq")

    let private applyBatch (dataSource: NpgsqlDataSource) (deltas: Map<AccountId, int64>) (newLastSeq: int64) : Task<unit> =
        let upsertParams =
            deltas
            |> Map.toList
            |> List.map (fun (AccountId accountId, delta) ->
                [ "account_id", Sql.string accountId
                  "delta", Sql.int64 delta ])
        let statements =
            [ "INSERT INTO ledger.account_balances (account_id, balance)
               VALUES (@account_id, @delta)
               ON CONFLICT (account_id) DO UPDATE SET balance = ledger.account_balances.balance + EXCLUDED.balance;",
              upsertParams
              "UPDATE ledger.account_balances_checkpoint SET last_seq = @last_seq WHERE id = 1;",
              [ [ "last_seq", Sql.int64 newLastSeq ] ] ]
        task {
            let! _ = dataSource |> Sql.fromDataSource |> Sql.executeTransactionAsync statements
            return ()
        }

    let rec private loop
        (dataSource: NpgsqlDataSource)
        (eventStore: EventStore)
        (pollInterval: TimeSpan)
        (stoppingToken: CancellationToken)
        (lastSeq: int64)
        : Task<unit> =
        task {
            let! newEvents = eventStore.ReadFrom lastSeq
            match newEvents with
            | [] ->
                do! Task.Delay(pollInterval, stoppingToken)
                return! loop dataSource eventStore pollInterval stoppingToken lastSeq
            | events ->
                let deltas = Fold.deltas events
                let newLastSeq = events |> List.map (fun e -> e.Seq) |> List.max
                do! applyBatch dataSource deltas newLastSeq
                return! loop dataSource eventStore pollInterval stoppingToken newLastSeq
        }

    let run
        (dataSource: NpgsqlDataSource)
        (eventStore: EventStore)
        (pollInterval: TimeSpan)
        (stoppingToken: CancellationToken)
        : Task<unit> =
        task {
            let! initialCheckpoint = readCheckpoint dataSource
            return! loop dataSource eventStore pollInterval stoppingToken initialCheckpoint
        }

    type HostedService(dataSource: NpgsqlDataSource, eventStore: EventStore, pollInterval: TimeSpan) =
        inherit BackgroundService()

        override _.ExecuteAsync(stoppingToken: CancellationToken) : Task =
            run dataSource eventStore pollInterval stoppingToken
