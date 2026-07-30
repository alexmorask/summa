module ApiTests

open System
open System.Net
open System.Net.Http
open System.Net.Http.Json
open System.Text
open System.Threading.Tasks
open Microsoft.AspNetCore.Hosting
open Microsoft.AspNetCore.Mvc.Testing
open Xunit
open Summa.Ledger.Api
open Summa.Ledger.Api.Program
open Summa.Ledger.Domain
open Summa.Ledger.Store

let private factory =
    (new WebApplicationFactory<Program>())
        .WithWebHostBuilder(fun builder -> builder.UseEnvironment("Development") |> ignore)

let private client = factory.CreateClient()

let private entry account direction amount : Entry =
    { Account = AccountId account; Direction = direction; Amount = amount }

let private request idempotencyKey entries : PostTransactionRequest =
    { OccurredAt = DateTimeOffset.UtcNow
      Description = "integration test transaction"
      CorrelationId = Guid.NewGuid().ToString()
      CausationId = Guid.NewGuid().ToString()
      IdempotencyKey = idempotencyKey
      Entries = entries }

let private balancedEntries =
    [ entry "ar:cust_integration" Debit 12000L
      entry "deferred_revenue" Credit 12000L ]

[<Fact>]
let ``GET health returns 200 OK`` () : Task =
    task {
        let! response = client.GetAsync("/health")
        Assert.Equal(HttpStatusCode.OK, response.StatusCode)
    }

[<Fact>]
let ``POST transactions with a balanced request returns 200 with a transaction id`` () : Task =
    task {
        let body = request (Guid.NewGuid().ToString()) balancedEntries

        let! response = client.PostAsJsonAsync("/transactions", body, Serialization.options)

        Assert.Equal(HttpStatusCode.OK, response.StatusCode)
        let! posted = response.Content.ReadFromJsonAsync<PostTransactionResponse>(Serialization.options)
        Assert.NotEqual(Guid.Empty, posted.Id)
    }

[<Fact>]
let ``POST transactions with the same idempotency key twice returns the same id`` () : Task =
    task {
        let body = request (Guid.NewGuid().ToString()) balancedEntries

        let! first = client.PostAsJsonAsync("/transactions", body, Serialization.options)
        let! firstPosted = first.Content.ReadFromJsonAsync<PostTransactionResponse>(Serialization.options)
        let! second = client.PostAsJsonAsync("/transactions", body, Serialization.options)
        let! secondPosted = second.Content.ReadFromJsonAsync<PostTransactionResponse>(Serialization.options)

        Assert.Equal(HttpStatusCode.OK, second.StatusCode)
        Assert.Equal(firstPosted.Id, secondPosted.Id)
    }

[<Fact>]
let ``POST transactions with unbalanced entries returns 400 with a domain error message`` () : Task =
    task {
        let unbalanced =
            [ entry "ar:cust_integration" Debit 12000L
              entry "deferred_revenue" Credit 11000L ]
        let body = request (Guid.NewGuid().ToString()) unbalanced

        let! response = client.PostAsJsonAsync("/transactions", body, Serialization.options)

        Assert.Equal(HttpStatusCode.BadRequest, response.StatusCode)
        let! error = response.Content.ReadFromJsonAsync<ErrorResponse>(Serialization.options)
        Assert.Equal("debits (12000) must equal credits (11000)", error.Error)
    }

[<Fact>]
let ``POST transactions with malformed JSON returns 400`` () : Task =
    task {
        use content = new StringContent("{not valid json", Encoding.UTF8, "application/json")
        let! response = client.PostAsync("/transactions", content)

        Assert.Equal(HttpStatusCode.BadRequest, response.StatusCode)
        let! error = response.Content.ReadFromJsonAsync<ErrorResponse>(Serialization.options)
        Assert.Equal("malformed JSON body", error.Error)
    }
