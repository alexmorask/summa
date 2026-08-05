namespace Summa.Recognition.Store

open System.Threading.Tasks
open Npgsql
open Npgsql.FSharp
open Summa.Recognition.Domain

module PostgresObligationEventStore =

    let private idempotencyKeyConstraint = "obligation_events_idempotency_key_key"

    let private append (dataSource: NpgsqlDataSource) (ObligationCreated obligation) : Task<AppendResult> =
        let (PerformanceObligationId rawId) = obligation.Id
        task {
            try
                let! seq =
                    dataSource
                    |> Sql.fromDataSource
                    |> Sql.query
                        "INSERT INTO recognition.obligation_events (obligation_id, correlation_id, causation_id, idempotency_key, payload)
                         VALUES (@obligation_id, @correlation_id, @causation_id, @idempotency_key, @payload)
                         RETURNING seq;"
                    |> Sql.parameters
                        [ "obligation_id", Sql.uuid rawId
                          "correlation_id", Sql.string obligation.CorrelationId
                          "causation_id", Sql.string obligation.CausationId
                          "idempotency_key", Sql.string obligation.IdempotencyKey
                          "payload", Sql.jsonb (Serialization.serialize obligation) ]
                    |> Sql.executeRowAsync (fun read -> read.int64 "seq")
                return Appended seq
            with :? PostgresException as ex when ex.SqlState = "23505" && ex.ConstraintName = idempotencyKeyConstraint ->
                let! existingId =
                    dataSource
                    |> Sql.fromDataSource
                    |> Sql.query "SELECT obligation_id FROM recognition.obligation_events WHERE idempotency_key = @idempotency_key;"
                    |> Sql.parameters [ "idempotency_key", Sql.string obligation.IdempotencyKey ]
                    |> Sql.executeRowAsync (fun read -> read.uuid "obligation_id")
                return Duplicate existingId
        }

    let private readFrom (dataSource: NpgsqlDataSource) (lastSeq: int64) : Task<StoredObligationEvent list> =
        dataSource
        |> Sql.fromDataSource
        |> Sql.query "SELECT seq, payload FROM recognition.obligation_events WHERE seq > @last_seq ORDER BY seq;"
        |> Sql.parameters [ "last_seq", Sql.int64 lastSeq ]
        |> Sql.executeAsync (fun read ->
            { Seq = read.int64 "seq"
              Event = ObligationCreated (Serialization.deserialize (read.text "payload")) })

    let create (dataSource: NpgsqlDataSource) : ObligationEventStore =
        { Append = append dataSource
          ReadFrom = readFrom dataSource }
