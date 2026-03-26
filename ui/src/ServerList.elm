module ServerList exposing (Action(..), Model, Msg(..), OAuthServer, init, serverConfigDecoder, update, view)

import Html exposing (..)
import Html.Attributes exposing (class)
import Html.Events exposing (onClick)
import Json.Decode as Decode


-- MODEL


type alias Client =
    { id : String
    , clientId : String
    , clientSecret : String
    }


type alias OAuthServer =
    { id : String
    , name : String
    , issuerUrl : String
    , authorizationUrl : String
    , tokenEndpoint : String
    , port_ : Int
    , running : Bool
    , clients : List Client
    }


type alias Model =
    { servers : List OAuthServer
    }


init : Model
init =
    { servers = []
    }



-- UPDATE


type Msg
    = OpenCreateForm
    | DeleteServer String
    | AddClient String
    | DeleteClient String
    | GotServerConfigs (Result Decode.Error (List OAuthServer))


type Action
    = RequestCreateForm Int
    | RequestDeleteServer String
    | RequestAddClient String
    | RequestDeleteClient String
    | NoAction


update : Msg -> Model -> ( Model, Action )
update msg model =
    case msg of
        OpenCreateForm ->
            ( model, RequestCreateForm (nextPort model) )

        DeleteServer id ->
            ( model, RequestDeleteServer id )

        AddClient serverId ->
            ( model, RequestAddClient serverId )

        DeleteClient clientId ->
            ( model, RequestDeleteClient clientId )

        GotServerConfigs result ->
            case result of
                Ok configs ->
                    ( { model | servers = configs }, NoAction )

                Err _ ->
                    ( model, NoAction )



-- PORTS / HELPERS


nextPort : Model -> Int
nextPort model =
    let
        basePort =
            9500

        usedPorts =
            List.map .port_ model.servers
    in
    findAvailable basePort usedPorts


findAvailable : Int -> List Int -> Int
findAvailable candidate used =
    if List.member candidate used then
        findAvailable (candidate + 1) used

    else
        candidate



-- DECODERS


clientDecoder : Decode.Decoder Client
clientDecoder =
    Decode.map3 Client
        (Decode.field "id" Decode.int |> Decode.map String.fromInt)
        (Decode.field "clientId" Decode.string)
        (Decode.field "clientSecret" Decode.string)


serverConfigDecoder : Decode.Decoder OAuthServer
serverConfigDecoder =
    Decode.map5
        (\id name authUrl tokUrl clients ->
            let
                port_ =
                    extractPort authUrl
            in
            { id = String.fromInt id
            , name = name
            , issuerUrl = "http://localhost:" ++ String.fromInt port_
            , authorizationUrl = authUrl
            , tokenEndpoint = tokUrl
            , port_ = port_
            , running = False
            , clients = clients
            }
        )
        (Decode.field "id" Decode.int)
        (Decode.field "configName" Decode.string)
        (Decode.field "authServerUrl" Decode.string)
        (Decode.field "tokenUrl" Decode.string)
        (Decode.field "clients" (Decode.list clientDecoder))


extractPort : String -> Int
extractPort url =
    url
        |> String.replace "http://localhost:" ""
        |> String.split "/"
        |> List.head
        |> Maybe.andThen String.toInt
        |> Maybe.withDefault 9500



-- VIEW


view : Model -> Html Msg
view model =
    div [ class "page-content" ]
        [ viewBody model
        , viewFooter
        ]


viewBody : Model -> Html Msg
viewBody model =
    if List.isEmpty model.servers then
        div [ class "empty-state" ]
            [ span [ class "empty-label" ] [ text "No servers configured" ] ]

    else
        div [ class "server-list" ]
            (List.map viewServerCard model.servers)


viewServerCard : OAuthServer -> Html Msg
viewServerCard server =
    div [ class "server-card" ]
        [ div [ class "server-card-header" ]
            [ div [ class "server-card-left" ]
                [ span [ class "server-name" ] [ text server.name ]
                , span [ class "server-meta" ] [ text ("localhost:" ++ String.fromInt server.port_) ]
                ]
            , div [ class "server-card-right" ]
                [ div [ class ("status-dot" ++ statusClass server.running) ] []
                , span [ class "status-label" ] [ text (statusLabel server.running) ]
                , button [ class "btn-delete", onClick (DeleteServer server.id) ] [ text "\u{1F5D1}" ]
                ]
            ]
        , div [ class "server-card-details" ]
            [ viewDetail "Issuer" server.issuerUrl
            , viewDetail "Authorization" server.authorizationUrl
            , viewDetail "Token" server.tokenEndpoint
            ]
        , div [ class "client-section" ]
            [ div [ class "client-section-header" ]
                [ span [ class "client-section-label" ] [ text "Clients" ]
                , button [ class "btn-add-client", onClick (AddClient server.id) ] [ text "+ Add Client" ]
                ]
            , if List.isEmpty server.clients then
                div [ class "client-empty" ] [ text "No clients" ]

              else
                div [ class "client-list" ]
                    (List.map viewClientCard server.clients)
            ]
        ]


viewClientCard : Client -> Html Msg
viewClientCard client =
    div [ class "client-card" ]
        [ div [ class "client-card-header" ]
            [ span [ class "client-label" ] [ text ("Client " ++ client.id) ]
            , button [ class "btn-delete", onClick (DeleteClient client.id) ] [ text "\u{1F5D1}" ]
            ]
        , div [ class "client-details" ]
            [ viewDetail "Client ID" client.clientId
            , viewDetail "Secret" client.clientSecret
            ]
        ]


viewDetail : String -> String -> Html Msg
viewDetail label value =
    div [ class "server-detail" ]
        [ span [ class "detail-label" ] [ text label ]
        , span [ class "detail-value" ] [ text value ]
        ]


statusClass : Bool -> String
statusClass running =
    if running then
        " running"

    else
        ""


statusLabel : Bool -> String
statusLabel running =
    if running then
        "running"

    else
        "stopped"


viewFooter : Html Msg
viewFooter =
    div [ class "footer" ]
        [ button [ class "btn-create", onClick OpenCreateForm ]
            [ text "+ Create new OAuth Server" ]
        ]
