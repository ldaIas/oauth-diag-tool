module ClientConfigList exposing (Action(..), ClientConfig, Model, Msg(..), clientConfigDecoder, init, update, view)

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


type alias Model =
    { configs : List ClientConfig
    }


init : Model
init =
    { configs = []
    }



-- UPDATE


type Msg
    = OpenCreateForm
    | DeleteConfig String
    | GotClientConfigs (Result Decode.Error (List ClientConfig))


type Action
    = RequestCreateForm
    | RequestDeleteConfig String
    | NoAction


update : Msg -> Model -> ( Model, Action )
update msg model =
    case msg of
        OpenCreateForm ->
            ( model, RequestCreateForm )

        DeleteConfig id ->
            ( model, RequestDeleteConfig id )

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
            (List.map viewConfigCard model.configs)


viewConfigCard : ClientConfig -> Html Msg
viewConfigCard config =
    div [ class "server-card" ]
        [ div [ class "server-card-header" ]
            [ div [ class "server-card-left" ]
                [ span [ class "server-name" ] [ text config.name ]
                , span [ class "server-meta" ] [ text config.grantType ]
                ]
            , div [ class "server-card-right" ]
                [ button [ class "btn-delete", onClick (DeleteConfig config.id) ] [ text "\u{1F5D1}" ]
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
