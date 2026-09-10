module Summa.Policy.Api.Program

open System
open System.Text.Json
open Falco
open Falco.Routing
open Microsoft.AspNetCore.Authentication.JwtBearer
open Microsoft.AspNetCore.Builder
open Microsoft.Extensions.Configuration
open Microsoft.Extensions.DependencyInjection
open Npgsql
open Summa.Policy.Domain
open Summa.Policy.Store

let private toErrorMessage : PricingError -> string =
    function
    | EmptySum -> "a Sum must contain at least one component"
    | EmptyUnitOfMeasure -> "a PerUnit component needs a unit of measure"
    | NegativeAmount -> "every amount must be zero or positive"
    | NegativeQuantity -> "every quantity must be zero or positive"
    | UnitMismatch (expected, actual) ->
        let (UnitOfMeasure expected) = expected
        let (UnitOfMeasure actual) = actual
        $"expected usage in '{expected}' but got '{actual}'"
    | QuantityCountMismatch (required, supplied) ->
        $"expected {required} usage quantities but got {supplied}"

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

let private publishPolicy (store: PolicyStore) : HttpHandler =
    fun ctx ->
        task {
            try
                let! request = Request.getJsonOptions<PublishPolicyRequest> Serialization.options ctx
                match Policy.create (Guid.NewGuid()) request.IdempotencyKey request.Pricing with
                | Error error -> return! errorResponse 400 (toErrorMessage error) ctx
                | Ok policy ->
                    let! result = store.Publish policy
                    let id =
                        match result with
                        | Published -> policy.Id
                        | Duplicate existingId -> existingId
                    return! Response.ofJsonOptions Serialization.options { Id = id } ctx
            with :? JsonException ->
                return! errorResponse 400 "malformed JSON body" ctx
        }

let private getPolicy (store: PolicyStore) (id: string) : HttpHandler =
    fun ctx ->
        task {
            match Guid.TryParse id with
            | false, _ -> return! errorResponse 400 "malformed policy id" ctx
            | true, policyId ->
                let! policy = store.Read policyId
                return!
                    match policy with
                    | Some policy -> Response.ofJsonOptions Serialization.options policy ctx
                    | None -> errorResponse 404 "no such policy" ctx
        }

type Program() = class end

[<EntryPoint>]
let main args =
    let builder = WebApplication.CreateBuilder(args)
    let connectionString = builder.Configuration.GetConnectionString("Summa")
    let dataSource = NpgsqlDataSource.Create(connectionString)
    let store = PostgresPolicyStore.create dataSource

    builder.Services
        .AddAuthentication(JwtBearerDefaults.AuthenticationScheme)
        .AddJwtBearer(fun options ->
            options.Authority <- builder.Configuration.["Authentication:Authority"]
            options.Audience <- builder.Configuration.["Authentication:Audience"])
    |> ignore

    let endpoints =
        [ get "/health" (Response.ofPlainText "OK")
          post "/policies" (requireRole "Policy.Write" (publishPolicy store))
          mapGet "/policies/{id}" (fun route -> route.GetString "id") (fun id -> requireRole "Policy.Read" (getPolicy store id)) ]

    let wapp = builder.Build()
    wapp.UseRouting()
        .Use(fun (app: IApplicationBuilder) -> app.UseAuthentication())
        .UseFalco(endpoints)
        .Run(Response.withStatusCode 404 >> Response.ofPlainText "not found")
    0
