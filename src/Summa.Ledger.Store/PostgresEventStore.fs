namespace Summa.Ledger.Store

open System.Threading.Tasks
open Npgsql
open Npgsql.FSharp
open Summa.Ledger.Domain

module PostgresEventStore =

    let private idempotencyKeyConstraint = "events_idempotency_key_key"

    let private append (dataSource: NpgsqlDataSource) (TransactionPosted transaction) : Task<AppendResult> =
        task {
            try
                let! seq =
                    dataSource
                    |> Sql.fromDataSource
                    |> Sql.query
                        "INSERT INTO ledger.events (transaction_id, occurred_at, correlation_id, causation_id, idempotency_key, payload)
                         VALUES (@transaction_id, @occurred_at, @correlation_id, @causation_id, @idempotency_key, @payload)
                         RETURNING seq;"
                    |> Sql.parameters
                        [ "transaction_id", Sql.uuid transaction.Id
                          "occurred_at", Sql.timestamptz transaction.OccurredAt
                          "correlation_id", Sql.string transaction.CorrelationId
                          "causation_id", Sql.string transaction.CausationId
                          "idempotency_key", Sql.string transaction.IdempotencyKey
                          "payload", Sql.jsonb (Serialization.serialize transaction) ]
                    |> Sql.executeRowAsync (fun read -> read.int64 "seq")
                return Appended seq
            with :? PostgresException as ex when ex.SqlState = "23505" && ex.ConstraintName = idempotencyKeyConstraint ->
                let! existingId =
                    dataSource
                    |> Sql.fromDataSource
                    |> Sql.query "SELECT transaction_id FROM ledger.events WHERE idempotency_key = @idempotency_key;"
                    |> Sql.parameters [ "idempotency_key", Sql.string transaction.IdempotencyKey ]
                    |> Sql.executeRowAsync (fun read -> read.uuid "transaction_id")
                return Duplicate existingId
        }

    let private readFrom (dataSource: NpgsqlDataSource) (lastSeq: int64) : Task<StoredEvent list> =
        dataSource
        |> Sql.fromDataSource
        |> Sql.query "SELECT seq, payload FROM ledger.events WHERE seq > @last_seq ORDER BY seq;"
        |> Sql.parameters [ "last_seq", Sql.int64 lastSeq ]
        |> Sql.executeAsync (fun read ->
            { Seq = read.int64 "seq"
              Event = TransactionPosted (Serialization.deserialize (read.text "payload")) })

    let create (dataSource: NpgsqlDataSource) : EventStore =
        { Append = append dataSource
          ReadFrom = readFrom dataSource }
