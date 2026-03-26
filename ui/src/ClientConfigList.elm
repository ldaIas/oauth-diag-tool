module ClientConfigList exposing (Action(..), AuthResult, ClientConfig, Model, Msg(..), authResultDecoder, clientConfigDecoder, init, update, view)

import Dict exposing (Dict)
import Html exposing (..)
import Html.Attributes exposing (class)
import Html.Events exposing (onClick)
import Json.Decode as Decode


-- MODEL


type alias ClientConfig =
    { id : String
    , name : String
    , issuerUrl : String
    , authorizationUrl : String
    , tokenUrl : String
    , clientId : String
    , clientSecret : String
    , scopes : String
    , grantType : String
    , extraParams : String
    }


type alias AuthResult =
    { configId : String
    , statusCode : Int
    , headers : List ( String, String )
    , body : String
    }


type AuthState
    = Idle
    | Loading
    | Done AuthResult
    | Error String


type alias Model =
    { configs : List ClientConfig
    , authStates : Dict String AuthState
    }


init : Model
init =
    { configs = []
    , authStates = Dict.empty
    }



-- UPDATE


type Msg
    = OpenCreateForm
    | DeleteConfig String
    | AuthorizeConfig String
    | GotAuthResult (Result Decode.Error AuthResult)
    | GotClientConfigs (Result Decode.Error (List ClientConfig))


type Action
    = RequestCreateForm
    | RequestDeleteConfig String
    | RequestAuthorizeConfig String
    | NoAction


update : Msg -> Model -> ( Model, Action )
update msg model =
    case msg of
        OpenCreateForm ->
            ( model, RequestCreateForm )

        DeleteConfig id ->
            ( model, RequestDeleteConfig id )

        AuthorizeConfig id ->
            ( { model | authStates = Dict.insert id Loading model.authStates }
            , RequestAuthorizeConfig id
            )

        GotAuthResult result ->
            case result of
                Ok authResult ->
                    ( { model | authStates = Dict.insert authResult.configId (Done authResult) model.authStates }
                    , NoAction
                    )

                Err _ ->
                    ( model, NoAction )

        GotClientConfigs result ->
            case result of
                Ok configs ->
                    ( { model | configs = configs }, NoAction )

                Err _ ->
                    ( model, NoAction )



-- DECODERS


clientConfigDecoder : Decode.Decoder ClientConfig
clientConfigDecoder =
    Decode.map8 ClientConfig
        (Decode.field "id" Decode.int |> Decode.map String.fromInt)
        (Decode.field "name" Decode.string)
        (Decode.field "issuerUrl" Decode.string)
        (Decode.field "authorizationUrl" Decode.string)
        (Decode.field "tokenUrl" Decode.string)
        (Decode.field "clientId" Decode.string)
        (Decode.field "clientSecret" Decode.string)
        (Decode.field "scopes" Decode.string)
        |> andMap (Decode.field "grantType" Decode.string)
        |> andMap (Decode.field "extraParams" Decode.string)


authResultDecoder : Decode.Decoder AuthResult
authResultDecoder =
    Decode.map4 AuthResult
        (Decode.field "configId" Decode.string)
        (Decode.field "statusCode" Decode.int)
        (Decode.field "headers" (Decode.list headerDecoder))
        (Decode.field "body" Decode.string)


headerDecoder : Decode.Decoder ( String, String )
headerDecoder =
    Decode.map2 Tuple.pair
        (Decode.index 0 Decode.string)
        (Decode.index 1 Decode.string)


andMap : Decode.Decoder a -> Decode.Decoder (a -> b) -> Decode.Decoder b
andMap =
    Decode.map2 (|>)



-- VIEW


view : Model -> Html Msg
view model =
    div [ class "page-content" ]
        [ viewBody model
        , viewFooter
        ]


viewBody : Model -> Html Msg
viewBody model =
    if List.isEmpty model.configs then
        div [ class "empty-state" ]
            [ span [ class "empty-label" ] [ text "No client configurations" ] ]

    else
        div [ class "server-list" ]
            (List.map (viewConfigCard model.authStates) model.configs)


viewConfigCard : Dict String AuthState -> ClientConfig -> Html Msg
viewConfigCard authStates config =
    let
        state =
            Dict.get config.id authStates |> Maybe.withDefault Idle
    in
    div [ class "server-card" ]
        [ div [ class "server-card-header" ]
            [ div [ class "server-card-left" ]
                [ span [ class "server-name" ] [ text config.name ]
                , span [ class "server-meta" ] [ text config.grantType ]
                ]
            , div [ class "server-card-right" ]
                [ viewAuthorizeButton state config.id
                , button [ class "btn-delete", onClick (DeleteConfig config.id) ] [ text "\u{1F5D1}" ]
                ]
            ]
        , div [ class "server-card-details" ]
            [ viewDetail "Issuer" config.issuerUrl
            , viewDetail "Authorization" config.authorizationUrl
            , viewDetail "Token" config.tokenUrl
            , viewDetail "Client ID" config.clientId
            , viewDetail "Secret" config.clientSecret
            , if String.isEmpty config.scopes then
                text ""

              else
                viewDetail "Scopes" config.scopes
            , if String.isEmpty config.extraParams then
                text ""

              else
                viewDetail "Extra Params" config.extraParams
            ]
        , viewAuthResponse state
        ]


viewAuthorizeButton : AuthState -> String -> Html Msg
viewAuthorizeButton state configId =
    case state of
        Loading ->
            button [ class "btn-server-control btn-authorize", Html.Attributes.disabled True ]
                [ text "Authorizing..." ]

        _ ->
            button [ class "btn-server-control btn-authorize", onClick (AuthorizeConfig configId) ]
                [ text "Authorize" ]


viewAuthResponse : AuthState -> Html Msg
viewAuthResponse state =
    case state of
        Idle ->
            text ""

        Loading ->
            div [ class "auth-response" ]
                [ span [ class "auth-loading" ] [ text "Requesting token..." ] ]

        Done result ->
            div [ class "auth-response" ]
                [ div [ class "auth-response-status" ]
                    [ span [ class "detail-label" ] [ text "Status" ]
                    , span [ class (statusClass result.statusCode) ] [ text (String.fromInt result.statusCode) ]
                    ]
                , div [ class "auth-response-headers" ]
                    [ span [ class "detail-label" ] [ text "Headers" ]
                    , div [ class "auth-headers-list" ]
                        (List.map viewHeader result.headers)
                    ]
                , div [ class "auth-response-body" ]
                    [ span [ class "detail-label" ] [ text "Body" ]
                    , pre [ class "auth-body-content" ] [ text result.body ]
                    ]
                ]

        Error err ->
            div [ class "auth-response" ]
                [ div [ class "auth-response-error" ]
                    [ span [ class "detail-label" ] [ text "Error" ]
                    , span [ class "auth-error-text" ] [ text err ]
                    ]
                ]


statusClass : Int -> String
statusClass code =
    if code >= 200 && code < 300 then
        "detail-value auth-status-ok"

    else
        "detail-value auth-status-error"


viewHeader : ( String, String ) -> Html Msg
viewHeader ( key, value ) =
    div [ class "auth-header-item" ]
        [ span [ class "auth-header-key" ] [ text key ]
        , span [ class "auth-header-value" ] [ text value ]
        ]


viewDetail : String -> String -> Html Msg
viewDetail label value =
    div [ class "server-detail" ]
        [ span [ class "detail-label" ] [ text label ]
        , span [ class "detail-value" ] [ text value ]
        ]


viewFooter : Html Msg
viewFooter =
    div [ class "footer" ]
        [ button [ class "btn-create", onClick OpenCreateForm ]
            [ text "+ Add Client Configuration" ]
        ]
