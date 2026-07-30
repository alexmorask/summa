FROM mcr.microsoft.com/dotnet/sdk:10.0 AS build
WORKDIR /src

COPY src/Summa.Ledger.Domain/Summa.Ledger.Domain.fsproj src/Summa.Ledger.Domain/
COPY src/Summa.Ledger.Store/Summa.Ledger.Store.fsproj src/Summa.Ledger.Store/
COPY src/Summa.Ledger.Api/Summa.Ledger.Api.fsproj src/Summa.Ledger.Api/
RUN dotnet restore src/Summa.Ledger.Api/Summa.Ledger.Api.fsproj

COPY src/Summa.Ledger.Domain/ src/Summa.Ledger.Domain/
COPY src/Summa.Ledger.Store/ src/Summa.Ledger.Store/
COPY src/Summa.Ledger.Api/ src/Summa.Ledger.Api/
RUN dotnet publish src/Summa.Ledger.Api/Summa.Ledger.Api.fsproj -c Release -o /app --no-restore

FROM mcr.microsoft.com/dotnet/aspnet:10.0 AS final
WORKDIR /app
COPY --from=build /app .
USER $APP_UID
EXPOSE 8080
ENTRYPOINT ["dotnet", "Summa.Ledger.Api.dll"]
