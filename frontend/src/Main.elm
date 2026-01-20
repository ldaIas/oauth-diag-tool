
import Browser
import Html.Styled exposing (..)
import Html.Styled.Attributes exposing (placeholder, type_, value)
import Html.Styled.Events exposing (onClick, onInput)
import Http
import Styles exposing (..)

main : Program () Model Msg
main =
    Browser.element
        { init = \_ -> ( init, Cmd.none )
        , update = update

        {- | Use toUnstyled because our view returns Html.Styled.Html and view need plain Html.
           | elm-css generates the css in the actual dom
        -}
        , view = view >> toUnstyled
        , subscriptions = \_ -> Sub.none
        }


type alias Model =
    { somethin : String
    }


init : Model
init =
    { somethin = ""
    }


update : Msg -> Model -> ( Model, Cmd Msg )
update msg model =
    (model, Cmd.none)

view : Model -> Html Msg
view model = 
    div [] []