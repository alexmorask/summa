namespace Summa.Policy.Store

open System
open System.Threading.Tasks
open Npgsql
open Npgsql.FSharp
open Summa.Policy.Domain

module PostgresPolicyStore =

    let private idempotencyKeyConstraint = "policies_idempotency_key_key"

    let private publish (dataSource: NpgsqlDataSource) (policy: Policy) : Task<PublishResult> =
        task {
            try
                let! _ =
                    dataSource
                    |> Sql.fromDataSource
                    |> Sql.query
                        "INSERT INTO policy.policies (policy_id, idempotency_key, pricing)
                         VALUES (@policy_id, @idempotency_key, @pricing);"
                    |> Sql.parameters
                        [ "policy_id", Sql.uuid policy.Id
                          "idempotency_key", Sql.string policy.IdempotencyKey
                          "pricing", Sql.jsonb (Serialization.serialize policy.Pricing) ]
                    |> Sql.executeNonQueryAsync
                return Published
            with :? PostgresException as ex when ex.SqlState = "23505" && ex.ConstraintName = idempotencyKeyConstraint ->
                let! existingId =
                    dataSource
                    |> Sql.fromDataSource
                    |> Sql.query "SELECT policy_id FROM policy.policies WHERE idempotency_key = @idempotency_key;"
                    |> Sql.parameters [ "idempotency_key", Sql.string policy.IdempotencyKey ]
                    |> Sql.executeRowAsync (fun read -> read.uuid "policy_id")
                return Duplicate existingId
        }

    let private read (dataSource: NpgsqlDataSource) (id: Guid) : Task<Policy option> =
        task {
            let! policies =
                dataSource
                |> Sql.fromDataSource
                |> Sql.query "SELECT policy_id, idempotency_key, pricing FROM policy.policies WHERE policy_id = @policy_id;"
                |> Sql.parameters [ "policy_id", Sql.uuid id ]
                |> Sql.executeAsync (fun read ->
                    { Id = read.uuid "policy_id"
                      IdempotencyKey = read.text "idempotency_key"
                      Pricing = Serialization.deserialize (read.text "pricing") })
            return List.tryHead policies
        }

    let create (dataSource: NpgsqlDataSource) : PolicyStore =
        { Publish = publish dataSource
          Read = read dataSource }
