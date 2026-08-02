module Summa.Ledger.Api.Program

open System
open System.Text.Json
open Falco
open Falco.Routing
open Microsoft.AspNetCore.Authentication.JwtBearer
open Microsoft.AspNetCore.Builder
open Microsoft.Extensions.Configuration
open Microsoft.Extensions.DependencyInjection
open Npgsql
open Npgsql.FSharp
open Summa.Ledger.Domain
open Summa.Ledger.Store

let private toErrorMessage : LedgerError -> string =
    function
    | TooFewEntries -> "a transaction needs at least two entries"
    | NonPositiveAmount -> "every entry amount must be positive"
    | Unbalanced (debits, credits) -> $"debits ({debits}) must equal credits ({credits})"

let private errorResponse statusCode message =
    Response.withStatusCode statusCode
    >> Response.ofJsonOptions Serialization.options { Error = message }

let private requireRole (role: string) (handler: HttpHandler) : HttpHandler =
    fun ctx ->
        if not ctx.User.Identity.IsAuthenticated then
            (Response.withStatusCode 401 >> Response.ofEmpty) ctx
        elif not (ctx.User.IsInRole role) then
            (Response.withStatusCode 403 >> Response.ofEmpty) ctx
        else
            handler ctx

let private postTransaction (store: EventStore) : HttpHandler =
    fun ctx ->
        task {
            try
                let! request = Request.getJsonOptions<PostTransactionRequest> Serialization.options ctx
                let command : PostTransaction =
                    { Id = Guid.NewGuid()
                      OccurredAt = request.OccurredAt
                      Description = request.Description
                      CorrelationId = request.CorrelationId
                      CausationId = request.CausationId
                      IdempotencyKey = request.IdempotencyKey
                      Entries = request.Entries }
                let! result = Handler.handle store command
                return!
                    match result with
                    | Ok id -> Response.ofJsonOptions Serialization.options { Id = id } ctx
                    | Error error -> errorResponse 400 (toErrorMessage error) ctx
            with :? JsonException ->
                return! errorResponse 400 "malformed JSON body" ctx
        }

let private getAccountBalance (dataSource: NpgsqlDataSource) (accountId: string) : HttpHandler =
    fun ctx ->
        task {
            let! rows =
                dataSource
                |> Sql.fromDataSource
                |> Sql.query "SELECT balance FROM ledger.account_balances WHERE account_id = @account_id;"
                |> Sql.parameters [ "account_id", Sql.string accountId ]
                |> Sql.executeAsync (fun read -> read.int64 "balance")
            return!
                match List.tryHead rows with
                | Some balance -> Response.ofJsonOptions Serialization.options { AccountId = accountId; Balance = balance } ctx
                | None -> errorResponse 404 "no such account" ctx
        }

let private getTrialBalance (dataSource: NpgsqlDataSource) : HttpHandler =
    fun ctx ->
        task {
            let! total =
                dataSource
                |> Sql.fromDataSource
                |> Sql.query "SELECT COALESCE(SUM(balance), 0)::BIGINT AS total FROM ledger.account_balances;"
                |> Sql.executeRowAsync (fun read -> read.int64 "total")
            return! Response.ofJsonOptions Serialization.options { Balance = total; Balanced = (total = 0L) } ctx
        }

type Program() = class end

[<EntryPoint>]
let main args =
    let builder = WebApplication.CreateBuilder(args)
    let connectionString = builder.Configuration.GetConnectionString("Summa")
    let dataSource = NpgsqlDataSource.Create(connectionString)
    let store = PostgresEventStore.create dataSource

    builder.Services
        .AddAuthentication(JwtBearerDefaults.AuthenticationScheme)
        .AddJwtBearer(fun options ->
            options.Authority <- builder.Configuration.["Authentication:Authority"]
            options.Audience <- builder.Configuration.["Authentication:Audience"])
    |> ignore

    let endpoints =
        [ get "/health" (Response.ofPlainText "OK")
          post "/transactions" (requireRole "Ledger.Write" (postTransaction store))
          mapGet "/accounts/{id}/balance" (fun route -> route.GetString "id") (fun id -> requireRole "Ledger.Read" (getAccountBalance dataSource id))
          get "/trial-balance" (requireRole "Ledger.Read" (getTrialBalance dataSource)) ]

    let wapp = builder.Build()
    wapp.UseRouting()
        .Use(fun (app: IApplicationBuilder) -> app.UseAuthentication())
        .UseFalco(endpoints)
        .Run(Response.withStatusCode 404 >> Response.ofPlainText "not found")
    0
